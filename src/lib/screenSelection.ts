// Screen selection in the Recordings timeline (Finder-style) and the clock
// helpers the time-range Delete/Strike uses: the screen side of
// lib/timelineSelection.ts, kept under its original names. Tested in
// screenSelection.test.ts (`npm test`).
//
// One rule keeps the count honest: the ids sent to the backend and the
// number shown in the selection bar both come from `orderedIds()`, which
// keeps only screens that are on the timeline right now, in timeline order.

export {
    EMPTY_SELECTION,
    clickItem as clickScreen,
    clockAt,
    durationLabel,
    orderedIds,
    parseClock,
    pruneSelection,
    rangeOfScreens,
    selectAll,
    selectLastMinutes,
    selectToEnd,
    spanLabel,
    summarize,
} from "./timelineSelection.ts";
export type { ClickMods, Selection, SelectionSummary, TimedItem as ScreenItem } from "./timelineSelection.ts";
