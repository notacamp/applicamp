import { queryOptions, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, CSRF_HEADERS, unwrap, type Schema } from "~/lib/api";

/** The profile with where it came from (`GET /api/v1/profile`). */
export type ProfileView = Schema<"ProfileView">;
/** The master profile. */
export type Profile = Schema<"Profile">;
export type Basics = Schema<"Basics">;
export type Work = Schema<"Work">;
export type Education = Schema<"Education">;
export type Skill = Schema<"Skill">;
export type Language = Schema<"Language">;
export type Certificate = Schema<"Certificate">;
export type Project = Schema<"Project">;
/** What can be imported (`linkedin_zip`, `pdf`, `text`). */
export type ImportKind = Schema<"ImportKind">;

export const PROFILE_QUERY_KEY = ["profile"] as const;

/** The signed-in person's profile. */
export const profileQuery = queryOptions({
  queryKey: PROFILE_QUERY_KEY,
  queryFn: async ({ signal }): Promise<ProfileView> =>
    unwrap(await api.GET("/api/v1/profile", { signal })),
});

export function useProfile() {
  return useQuery(profileQuery);
}

/** An empty profile, as the server sends one. */
export const EMPTY_PROFILE: Profile = {
  basics: { profiles: [] },
  work: [],
  education: [],
  skills: [],
  languages: [],
  certificates: [],
  projects: [],
};

/** Whether `value` says something (not missing, not blank). */
export function present(value: string | null | undefined): value is string {
  return value !== undefined && value !== null && value.trim() !== "";
}

/** Whether `profile` holds nothing at all. */
export function isEmptyProfile(profile: Profile): boolean {
  const { basics } = profile;
  const lists = [
    basics.profiles,
    profile.work,
    profile.education,
    profile.skills,
    profile.languages,
    profile.certificates,
    profile.projects,
  ];
  const values = [
    basics.name,
    basics.label,
    basics.email,
    basics.location,
    basics.summary,
    basics.url,
  ];
  return lists.every((list) => list.length === 0) && !values.some((value) => present(value));
}

/** Saves the whole profile (`PUT /api/v1/profile`); the page shows the saved one. */
export function useSaveProfile() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (profile: Profile): Promise<ProfileView> =>
      unwrap(await api.PUT("/api/v1/profile", { headers: CSRF_HEADERS, body: profile })),
    onSuccess: (view) => {
      queryClient.setQueryData(PROFILE_QUERY_KEY, view);
    },
  });
}
