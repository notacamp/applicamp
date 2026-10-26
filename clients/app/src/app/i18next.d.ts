// Types `t()` keys against the English messages, one line per namespace.
import "i18next";

import type { ShortcutMessages } from "~/lib/shortcuts";
import type { UiMessages } from "~/lib/ui";
import type identity from "~/modules/identity/i18n/en.json";
import type profile from "~/modules/profile/i18n/en.json";

import type shell from "./i18n/en.json";

declare module "i18next" {
  interface CustomTypeOptions {
    defaultNS: "shell";
    resources: {
      shell: typeof shell;
      ui: UiMessages;
      shortcuts: ShortcutMessages;
      identity: typeof identity;
      profile: typeof profile;
    };
  }
}
