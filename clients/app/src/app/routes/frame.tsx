import { Outlet, useLocation, useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import { useShortcut, useShortcuts } from "~/lib/shortcuts";
import {
  AppBrand,
  CommandMenu,
  IconButton,
  NavItem,
  PhoneHeader,
  Shell,
  ShortcutSheet,
  Sidebar,
  SidebarFooter,
  Spacer,
  TabBar,
  Tagged,
} from "~/lib/ui";
import { AccountMenu, SessionGate, useMe, useSignOut } from "~/modules/identity";

import { PLACES, placeOf, useInAppLink, type PlacePath } from "../links";

/** Every signed-in page: the session gate, then the app frame around it. */
export function SignedInFrame() {
  return (
    <SessionGate>
      <Frame />
    </SessionGate>
  );
}

/** Where a key can take you: the three places and the pages reached from them. */
type Address = PlacePath | "/boards" | "/companies" | "/cvs" | "/agents";

/**
 * The app frame: the sidebar (Search, then Today, Jobs and You, and the
 * account), or the phone header and tab bar below 48rem; ⌘K's command menu
 * and the `?` sheet. No key is hinted on a nav item or a button: the app is
 * for people who are not tech-savvy. The keys keep working silently, and the
 * pages that are not places (Job sites, Companies, Agents, CVs) stay a
 * command in Search.
 */
function Frame() {
  const { t } = useTranslation();
  const { t: tNav } = useTranslation("shortcuts");
  const navigate = useNavigate();
  const { pathname } = useLocation();
  const { registry } = useShortcuts();
  const me = useMe();
  const signOut = useSignOut();
  const link = useInAppLink();
  const [menuOpen, setMenuOpen] = useState(false);
  const [sheetOpen, setSheetOpen] = useState(false);
  const current = placeOf(pathname);

  const go = (to: Address) => () => {
    void navigate({ to });
  };
  useShortcut("nav.today", go("/"));
  useShortcut("nav.pipeline", go("/pipeline"));
  useShortcut("nav.boards", go("/boards"));
  useShortcut("nav.companies", go("/companies"));
  useShortcut("nav.cvs", go("/cvs"));
  useShortcut("nav.agents", go("/agents"));
  useShortcut("nav.profile", go("/profile"));
  // ⌘K's "Search now" and "New opportunity" (and their keys) from any page:
  // Jobs starts the search, or opens New, once it is shown.
  useShortcut("sources.start", () => {
    void navigate({ to: "/pipeline", search: { run: "search" } });
  });
  useShortcut("pipeline.add", () => {
    void navigate({ to: "/pipeline", search: { run: "add" } });
  });
  useShortcut("command.open", () => {
    setSheetOpen(false);
    setMenuOpen((open) => !open);
  });
  useShortcut("sheet.open", () => {
    setMenuOpen(false);
    setSheetOpen((open) => !open);
  });

  const places = PLACES.map((place) => ({
    ...place,
    label: tNav(`nav.${place.id}`),
    current: place.id === current,
  }));

  return (
    <>
      <Shell>
        <PhoneHeader>
          <AppBrand name={t("appName")} />
          <Spacer />
          <IconButton
            size="lg"
            icon="search"
            label={t("frame.jump")}
            data-testid="jump-phone"
            onClick={() => {
              registry.run("command.open");
            }}
          />
          <AccountMenu compact testId="account-menu-phone" />
        </PhoneHeader>
        <Sidebar label={t("frame.places")}>
          <AppBrand name={t("appName")} detail={me.data?.email} place="sidebar" />
          <Tagged testId="jump">
            <NavItem
              label={t("frame.jump")}
              icon="search"
              onClick={() => {
                registry.run("command.open");
              }}
            />
          </Tagged>
          {places.map((place) => (
            <Tagged key={place.id} testId={`nav-${place.id}`}>
              <NavItem
                label={place.label}
                icon={place.icon}
                current={place.current}
                {...link(place.path)}
              />
            </Tagged>
          ))}
          <SidebarFooter>
            <AccountMenu testId="account-menu" />
          </SidebarFooter>
        </Sidebar>
        <Outlet />
      </Shell>
      <Tagged testId="tab-bar">
        <TabBar
          phoneOnly
          label={t("frame.places")}
          items={places.map((place) => ({
            id: place.id,
            label: place.label,
            icon: place.icon,
            current: place.current,
            ...link(place.path),
          }))}
        />
      </Tagged>
      <ShortcutSheet
        open={sheetOpen}
        onClose={() => {
          setSheetOpen(false);
        }}
      />
      <CommandMenu
        open={menuOpen}
        onClose={() => {
          setMenuOpen(false);
        }}
        commands={[
          {
            id: "go.cvs",
            label: tNav("nav.cvs"),
            group: t("commands.go"),
            shortcut: "nav.cvs",
            onSelect: go("/cvs"),
          },
          {
            id: "account.sign-out",
            label: t("commands.signOut"),
            group: t("commands.account"),
            onSelect: () => {
              signOut.mutate();
            },
          },
        ]}
      />
    </>
  );
}
