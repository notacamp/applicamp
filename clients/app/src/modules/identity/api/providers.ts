import { useQuery } from "@tanstack/react-query";

import { api, unwrap } from "~/lib/api";

import type { Provider } from "./me";

/** The sign-in providers this server offers (`GET /bff/providers`), in order. */
export function useProviders() {
  return useQuery({
    queryKey: ["identity", "providers"],
    queryFn: async ({ signal }): Promise<Provider[]> =>
      unwrap(await api.GET("/bff/providers", { signal })).providers,
    staleTime: 5 * 60_000,
  });
}

/** Each provider's own name, the same in every language. */
export const PROVIDER_NAMES: Record<Provider, string> = {
  campbooks: "Campbooks",
  linkedin: "LinkedIn",
};

/** Where a sign-in with `provider` starts: the server's BFF (a full navigation). */
export function signInUrl(provider: Provider): string {
  return `/bff/sign-in/${provider}`;
}
