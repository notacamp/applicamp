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
export { InlineEdit, type InlineEditProps } from "./components/InlineEdit/InlineEdit";
export { InlineText, type InlineTextProps } from "./components/InlineText/InlineText";
export { ListField, withItem, type ListFieldProps } from "./components/ListField/ListField";
export {
  Cluster,
  Section,
  Stack,
  type SectionProps,
  type StackProps,
} from "./components/Layout/Layout";
export {
  Board,
  BoardCard,
  BoardColumn,
  type BoardCardProps,
  type BoardColumnProps,
  type BoardProps,
} from "./components/Board/Board";
export { Bullets } from "./components/Bullets/Bullets";
export { Chat, ChatBubble, type ChatBubbleProps } from "./components/Chat/Chat";
export {
  ChoiceNav,
  type ChoiceNavItem,
  type ChoiceNavProps,
} from "./components/ChoiceNav/ChoiceNav";
export {
  FactBlock,
  FactRow,
  type FactBlockProps,
  type FactRowProps,
} from "./components/FactBlock/FactBlock";
export { StatusMark, type StatusMarkProps } from "./components/StatusMark/StatusMark";
export { Workbench, type WorkbenchProps } from "./components/Workbench/Workbench";
export { CardList, type CardListProps } from "./components/CardList/CardList";
export { DataTable, type DataColumn, type DataTableProps } from "./components/DataTable/DataTable";
export { Label, LABEL_HUES, type LabelHue, type LabelProps } from "./components/Label/Label";
export { MatchBar, type MatchBarProps } from "./components/MatchBar/MatchBar";
export { Page, type PageProps } from "./components/Page/Page";
export { SidePeek, type SidePeekProps } from "./components/SidePeek/SidePeek";
export { NARROW_QUERY, useNarrowScreen } from "./hooks/narrow";
export {
  PdfViewer,
  type PdfViewerLabels,
  type PdfViewerProps,
} from "./components/PdfViewer/PdfViewer";
export { Placeholder, type PlaceholderProps } from "./components/Placeholder/Placeholder";
export {
  RowItem,
  RowList,
  type RowItemProps,
  type RowListProps,
} from "./components/RowList/RowList";
export { Property, PropertyList, type PropertyProps } from "./components/Properties/Properties";
export {
  PickList,
  PickRow,
  type PickListProps,
  type PickRowProps,
} from "./components/PickList/PickList";
export { SplitView, type SplitViewProps } from "./components/SplitView/SplitView";
export {
  EventLine,
  EventLog,
  type EventLineProps,
  type EventLogProps,
} from "./components/EventLog/EventLog";
export { Tagged } from "./components/Tagged/Tagged";
export { TextArea, type TextAreaProps } from "./components/TextArea/TextArea";
export {
  ViewSwitch,
  type ViewSwitchOption,
  type ViewSwitchProps,
} from "./components/ViewSwitch/ViewSwitch";
export {
  ChipList,
  CvLine,
  CvLines,
  Marked,
  markedParts,
  Paper,
  PaperHead,
  PaperRole,
  PaperSection,
  ReviewLayout,
  type ReviewLayoutProps,
} from "./components/Cv/Cv";
