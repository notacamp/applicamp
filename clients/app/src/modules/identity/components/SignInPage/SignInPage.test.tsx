import { screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { renderWithProviders, stubApi } from "../../../../../test/render";

import { SignInPage } from "./SignInPage";

describe("the sign-in page", () => {
  it("offers one button per configured provider, the first one primary", async () => {
    stubApi({ "GET /bff/providers": { body: { providers: ["linkedin", "campbooks"] } } });
    renderWithProviders(<SignInPage />);

    expect(screen.getByTestId("sign-in-loading")).toHaveTextContent("Loading");
    const linkedin = await screen.findByTestId("sign-in-linkedin");
    expect(linkedin).toHaveTextContent("Continue with LinkedIn");
    expect(linkedin).toHaveAttribute("href", "/bff/sign-in/linkedin");
    expect(screen.getByTestId("sign-in-campbooks")).toHaveAttribute(
      "href",
      "/bff/sign-in/campbooks",
    );
    expect(screen.queryByTestId("sign-in-error")).toBeNull();
    expect(screen.getByTestId("sign-in")).toHaveTextContent("Sign in to Applicamp");
  });

  it("says why the last sign-in failed", async () => {
    stubApi({ "GET /bff/providers": { body: { providers: ["campbooks"] } } });
    renderWithProviders(<SignInPage error="sign_in_failed" />);
    expect(await screen.findByTestId("sign-in-error")).toHaveTextContent(
      "That sign-in didn’t work",
    );
  });

  it("says when no way in is set up", async () => {
    stubApi({ "GET /bff/providers": { body: { providers: [] } } });
    renderWithProviders(<SignInPage />);
    expect(await screen.findByTestId("sign-in-none")).toHaveTextContent(
      "No way to sign in is set up yet",
    );
  });

  it("offers to try again when the server cannot be reached", async () => {
    let up = false;
    stubApi({
      "GET /bff/providers": () =>
        up ? { body: { providers: ["campbooks"] } } : { status: 502, body: { code: "x" } },
    });
    const user = userEvent.setup();
    renderWithProviders(<SignInPage />);

    expect(await screen.findByTestId("sign-in-unreachable")).toHaveTextContent(
      "Applicamp can’t be reached",
    );
    up = true;
    await user.click(screen.getByTestId("sign-in-retry"));
    expect(await screen.findByTestId("sign-in-campbooks")).toBeInTheDocument();
  });

  it("speaks Portuguese", async () => {
    stubApi({ "GET /bff/providers": { body: { providers: ["campbooks"] } } });
    renderWithProviders(<SignInPage error="provider_unavailable" />, { locale: "pt-PT" });
    expect(await screen.findByTestId("sign-in-campbooks")).toHaveTextContent(
      "Continuar com Campbooks",
    );
    expect(screen.getByTestId("sign-in-error")).toHaveTextContent("não está a responder");
  });
});
