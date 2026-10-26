// Applicamp's design system: @notacamp/ui (the Not A Camp components and
// tokens) with Applicamp's token set over it, its preferences, its strings
// and the few compositions the package has no component for yet. Modules
// and the app shell import UI only from here (dependency-cruiser:
// "ui-public-surface"), and only here may class names and styles appear.
import "@notacamp/ui/tokens.css";
import "@notacamp/ui/base.css";
import "@notacamp/ui/ui.css";
import "~/lib/theme/applicamp.css";

export * from "@notacamp/ui";
// The app's own wiring of the package: its preferences, its strings.
export { APP_THEME_PREFS, ThemeProvider } from "./theme";
export { UI_NAMESPACES } from "./messages";

// Compositions of package primitives, candidates to move into @notacamp/ui.
export {
  AppBrand,
  AppMark,
  type AppBrandProps,
  type AppMarkProps,
} from "./components/AppMark/AppMark";
export { AppFrame } from "./components/AppFrame/AppFrame";
export { FileField, type FileFieldProps } from "./components/FileField/FileField";
export { InlineText, type InlineTextProps } from "./components/InlineText/InlineText";
export {
  Cluster,
  Section,
  Stack,
  type SectionProps,
  type StackProps,
} from "./components/Layout/Layout";
export { Page, type PageProps } from "./components/Page/Page";
export { Placeholder, type PlaceholderProps } from "./components/Placeholder/Placeholder";
export { Property, PropertyList, type PropertyProps } from "./components/Properties/Properties";
export { Tagged } from "./components/Tagged/Tagged";
export { TextArea, type TextAreaProps } from "./components/TextArea/TextArea";
