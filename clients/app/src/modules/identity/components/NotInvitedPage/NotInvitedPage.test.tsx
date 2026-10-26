import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { renderWithProviders } from "../../../../../test/render";

import { NotInvitedPage } from "./NotInvitedPage";

describe("the not-invited page", () => {
  it("says the person is not on the list and leads back to the sign-in page", () => {
    renderWithProviders(<NotInvitedPage />, { locale: "es-ES" });
    expect(screen.getByTestId("not-invited")).toHaveTextContent("Todavía no estás en la lista");
    expect(screen.getByTestId("back-to-sign-in")).toHaveAttribute("href", "/sign-in");
  });
});
