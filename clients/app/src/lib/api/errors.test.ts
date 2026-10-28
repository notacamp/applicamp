import { describe, expect, it } from "vitest";

import { ApiError, errorStatus, problemCode, problemDetail, unwrap, unwrapEmpty } from "./errors";

const response = (status: number) => new Response(null, { status });

describe("API results", () => {
  it("hands back the data of a successful call", () => {
    expect(unwrap({ data: { ok: true }, response: response(200) })).toEqual({ ok: true });
    expect(() => {
      unwrapEmpty({ response: response(204) });
    }).not.toThrow();
  });

  it("turns a failed call into an ApiError with its problem", () => {
    const problem = { code: "session_required", status: 401 };
    let caught: unknown;
    try {
      unwrap({ error: problem, response: response(401) });
    } catch (error) {
      caught = error;
    }
    expect(caught).toBeInstanceOf(ApiError);
    expect(errorStatus(caught)).toBe(401);
    expect(problemCode(caught)).toBe("session_required");
    expect(() => {
      unwrapEmpty({ error: { code: "csrf_check_failed" }, response: response(403) });
    }).toThrow(ApiError);
  });

  it("knows no code or status for anything else", () => {
    expect(problemCode(new Error("network"))).toBeUndefined();
    expect(problemCode(new ApiError(500, "oops"))).toBeUndefined();
    expect(problemCode(new ApiError(500, { code: 5 }))).toBeUndefined();
    expect(problemCode(new ApiError(500, null))).toBeUndefined();
    expect(errorStatus(new Error("network"))).toBeUndefined();
  });

  it("carries the server's own words about what went wrong", () => {
    const problem = { code: "invalid_config", detail: "`max_pages` must be 1 to 10" };
    expect(problemDetail(new ApiError(400, problem))).toBe("`max_pages` must be 1 to 10");
    expect(problemDetail(new ApiError(400, { code: "x", detail: " " }))).toBeUndefined();
    expect(problemDetail(new ApiError(400, { code: "x" }))).toBeUndefined();
    expect(problemDetail(new ApiError(500, null))).toBeUndefined();
    expect(problemDetail(new Error("network"))).toBeUndefined();
  });
});
