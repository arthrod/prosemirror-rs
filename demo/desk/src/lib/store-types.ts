import type { ReviewDoc } from "./review/types.ts";

export type LibraryFile = {
  id: string;
  parentId: string | null;
  name: string;
  kind: "folder" | "markdown" | "plain";
  body: string;
  frontmatter: string;
  review: ReviewDoc;
  created: string;
  updated: string;
};

export type WidthMode = "standard" | "wide" | "full";
export type ThemeMode = "system" | "light" | "dark";
export type MobilePane = "tree" | "write" | "review";

export type Prefs = {
  theme: ThemeMode;
  zoom: number;
  width: WidthMode;
  showTree: boolean;
  showReview: boolean;
  showModel: boolean;
  showSource: boolean;
  collab: boolean;
  allowRemote: boolean;
  mobilePane: MobilePane;
};

export type StepRecord = { label: string; json: unknown };

export const defaultPrefs: Prefs = {
  theme: "system",
  zoom: 1,
  width: "standard",
  showTree: true,
  showReview: true,
  showModel: false,
  showSource: false,
  collab: false,
  allowRemote: false,
  mobilePane: "write",
};
