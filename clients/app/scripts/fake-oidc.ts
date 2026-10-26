// A tiny fake OpenID Connect provider, standing in for Campbooks in the e2e
// stack and on a laptop: discovery, an authorization endpoint that approves at
// once (no login form), the token endpoint (PKCE S256, client_secret_post) and
// the key set. ID tokens are EdDSA, with the nonce, audience and issuer the
// server checks. Never for production: anyone who reaches it signs in.
//
//   node scripts/fake-oidc.ts
//
// Settings (environment):
//   FAKE_OIDC_PORT     port to listen on (9010)
//   FAKE_OIDC_HOST     address to bind (127.0.0.1; 0.0.0.0 in a container)
//   FAKE_OIDC_ISSUER   its issuer URL, as the server and the browser reach it
//                      (http://localhost:<port>)
//   FAKE_OIDC_CLIENTS  the clients it knows, `id:secret,id:secret`
//                      (applicamp-dev:dev-only-secret)
//   FAKE_OIDC_EMAIL    who signs in (demo@example.com), always verified
//   FAKE_OIDC_NAME     their name (Alex Demo)
//
// Point the server at it with APPLICAMP_OIDC__CAMPBOOKS__ISSUER=<issuer>,
// APPLICAMP_OIDC__CAMPBOOKS__CLIENT_ID and APPLICAMP_OIDC__CAMPBOOKS__CLIENT_SECRET.
import { createHash, generateKeyPairSync, randomBytes, sign } from "node:crypto";
import { createServer, type IncomingMessage, type ServerResponse } from "node:http";

const env = process.env;
const port = Number(env["FAKE_OIDC_PORT"] ?? "9010");
const host = env["FAKE_OIDC_HOST"] ?? "127.0.0.1";
const issuer = (env["FAKE_OIDC_ISSUER"] ?? `http://localhost:${String(port)}`).replace(/\/+$/, "");
const email = env["FAKE_OIDC_EMAIL"] ?? "demo@example.com";
const name = env["FAKE_OIDC_NAME"] ?? "Alex Demo";
const clients = new Map(
  (env["FAKE_OIDC_CLIENTS"] ?? "applicamp-dev:dev-only-secret")
    .split(",")
    .map((pair) => pair.trim().split(":"))
    .filter((pair): pair is [string, string] => pair.length === 2),
);

const KID = "fake-oidc-1";
const CODE_LIFETIME_MS = 60_000;
const { privateKey, publicKey } = generateKeyPairSync("ed25519");
const jwk = { ...publicKey.export({ format: "jwk" }), kid: KID, use: "sig", alg: "EdDSA" };

interface Grant {
  clientId: string;
  redirectUri: string;
  nonce: string;
  challenge: string;
  expiresAt: number;
}
const grants = new Map<string, Grant>();

const base64url = (data: Buffer | string) => Buffer.from(data).toString("base64url");

/** A signed ID token for `grant`. */
function idToken(grant: Grant): string {
  const now = Math.floor(Date.now() / 1000);
  const header = base64url(JSON.stringify({ alg: "EdDSA", typ: "JWT", kid: KID }));
  const claims = base64url(
    JSON.stringify({
      iss: issuer,
      sub: `fake-${createHash("sha256").update(email).digest("hex").slice(0, 16)}`,
      aud: grant.clientId,
      azp: grant.clientId,
      iat: now,
      exp: now + 300,
      nonce: grant.nonce,
      email,
      email_verified: true,
      name,
    }),
  );
  const signature = sign(null, Buffer.from(`${header}.${claims}`), privateKey);
  return `${header}.${claims}.${base64url(signature)}`;
}

function json(response: ServerResponse, status: number, body: unknown) {
  response.writeHead(status, { "content-type": "application/json", "cache-control": "no-store" });
  response.end(JSON.stringify(body));
}

async function form(request: IncomingMessage): Promise<URLSearchParams> {
  let body = "";
  for await (const chunk of request) {
    body += String(chunk);
    if (body.length > 64 * 1024) {
      break;
    }
  }
  return new URLSearchParams(body);
}

function authorize(url: URL, response: ServerResponse) {
  const query = url.searchParams;
  const clientId = query.get("client_id") ?? "";
  const redirectUri = query.get("redirect_uri") ?? "";
  if (!clients.has(clientId) || !/^https?:\/\//.test(redirectUri)) {
    json(response, 400, {
      error: "invalid_request",
      error_description: "unknown client or redirect",
    });
    return;
  }
  const back = new URL(redirectUri);
  const state = query.get("state");
  if (state !== null) {
    back.searchParams.set("state", state);
  }
  if (query.get("response_type") !== "code" || query.get("code_challenge_method") !== "S256") {
    back.searchParams.set("error", "unsupported_response_type");
  } else {
    const code = randomBytes(24).toString("base64url");
    grants.set(code, {
      clientId,
      redirectUri,
      nonce: query.get("nonce") ?? "",
      challenge: query.get("code_challenge") ?? "",
      expiresAt: Date.now() + CODE_LIFETIME_MS,
    });
    back.searchParams.set("code", code);
  }
  console.log(`fake-oidc: approved a sign-in for ${clientId}`);
  response.writeHead(302, { location: back.toString(), "cache-control": "no-store" });
  response.end();
}

async function token(request: IncomingMessage, response: ServerResponse) {
  const params = await form(request);
  const clientId = params.get("client_id") ?? "";
  if (clients.get(clientId) !== params.get("client_secret")) {
    json(response, 401, { error: "invalid_client" });
    return;
  }
  const code = params.get("code") ?? "";
  const grant = grants.get(code);
  grants.delete(code);
  const verifier = params.get("code_verifier") ?? "";
  const challenge = createHash("sha256").update(verifier).digest("base64url");
  if (
    params.get("grant_type") !== "authorization_code" ||
    grant === undefined ||
    grant.expiresAt < Date.now() ||
    grant.clientId !== clientId ||
    grant.redirectUri !== params.get("redirect_uri") ||
    grant.challenge !== challenge
  ) {
    json(response, 400, { error: "invalid_grant" });
    return;
  }
  json(response, 200, {
    access_token: randomBytes(24).toString("base64url"),
    token_type: "Bearer",
    expires_in: 3600,
    scope: "openid profile email",
    id_token: idToken(grant),
  });
}

const server = createServer((request, response) => {
  const url = new URL(request.url ?? "/", issuer);
  const route = `${request.method ?? "GET"} ${url.pathname}`;
  switch (route) {
    case "GET /.well-known/openid-configuration": {
      json(response, 200, {
        issuer,
        authorization_endpoint: `${issuer}/authorize`,
        token_endpoint: `${issuer}/token`,
        jwks_uri: `${issuer}/jwks`,
        response_types_supported: ["code"],
        subject_types_supported: ["public"],
        id_token_signing_alg_values_supported: ["EdDSA"],
        code_challenge_methods_supported: ["S256"],
        token_endpoint_auth_methods_supported: ["client_secret_post"],
        scopes_supported: ["openid", "profile", "email", "offline_access"],
      });
      return;
    }
    case "GET /authorize": {
      authorize(url, response);
      return;
    }
    case "POST /token": {
      token(request, response).catch(() => {
        json(response, 500, { error: "server_error" });
      });
      return;
    }
    case "GET /jwks": {
      json(response, 200, { keys: [jwk] });
      return;
    }
    case "GET /healthz": {
      json(response, 200, { status: "ok" });
      return;
    }
    default: {
      json(response, 404, { error: "not_found" });
    }
  }
});

server.listen(port, host, () => {
  console.log(`fake-oidc: ${issuer} (listening on ${host}:${String(port)}), signs in ${email}`);
});

for (const signal of ["SIGINT", "SIGTERM"] as const) {
  process.on(signal, () => {
    server.closeAllConnections();
    server.close();
  });
}
