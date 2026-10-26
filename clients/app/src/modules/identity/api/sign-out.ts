import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";

import { api, CSRF_HEADERS, unwrapEmpty } from "~/lib/api";

/**
 * Signs out (`POST /bff/sign-out`): the server deletes the session and
 * clears its cookie. Unsafe, so it carries the CSRF header.
 */
export async function signOut(): Promise<void> {
  unwrapEmpty(await api.POST("/bff/sign-out", { headers: CSRF_HEADERS }));
}

/**
 * Signing out, then the sign-in page, with nothing of the account left in
 * the page's cache for whoever uses this browser next.
 */
export function useSignOut() {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  return useMutation({
    mutationFn: signOut,
    onSuccess: async () => {
      // Forget first: the sign-in page then starts from an empty cache.
      queryClient.clear();
      await navigate({ to: "/sign-in", replace: true });
    },
  });
}
