import { Outlet, useLocation, useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import { useShortcut, useShortcuts } from "~/lib/shortcuts";
import {
  AppBrand,
  AppFrame,
  CommandMenu,
  IconButton,
  NavItem,
  PhoneHeader,
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

/**
 * The app frame: the sidebar (Today, Pipeline, Agents, Profile & goals, the
 * shortcuts and the account), or the phone header and tab bar below 48rem;
 * ⌘K's command menu and the `?` sheet.
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

  const go = (to: PlacePath) => () => {
    void navigate({ to });
  };
  useShortcut("nav.today", go("/"));
  useShortcut("nav.pipeline", go("/pipeline"));
  useShortcut("nav.agents", go("/agents"));
  useShortcut("nav.profile", go("/profile"));
  // ⌘K's "Search now" and "New opportunity" (and their keys) from any page:
  // the Pipeline starts the search, or opens New, once it is shown.
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
      <AppFrame>
        <PhoneHeader>
          <AppBrand name={t("appName")} />
          <Spacer />
          <IconButton
            size="lg"
            icon="search"
            label={t("frame.jump")}
            shortcut="command.open"
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
              shortcut="command.open"
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
                shortcut={place.shortcut}
                current={place.current}
                {...link(place.path)}
              />
            </Tagged>
          ))}
          <SidebarFooter>
            <Tagged testId="shortcut-sheet-button">
              <NavItem
                label={tNav("sheet.open")}
                icon="keyboard"
                shortcut="sheet.open"
                onClick={() => {
                  registry.run("sheet.open");
                }}
              />
            </Tagged>
            <AccountMenu testId="account-menu" />
          </SidebarFooter>
        </Sidebar>
        <Outlet />
      </AppFrame>
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
