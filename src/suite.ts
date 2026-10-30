export interface SuiteApp {
  name: string;
  repository: string;
}

export const SUITE_APPS: readonly SuiteApp[] = [
  { name: "FinFetcher", repository: "mkiera/FinFetcher" },
  { name: "FlipperClipper", repository: "mkiera/FlipperClipper" },
  { name: "BlubberBound", repository: "mkiera/BlubberBound" },
];
