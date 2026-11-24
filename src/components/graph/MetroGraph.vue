<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { GraphMeta, GraphRow, Lane, RefInfo } from "@/lib/api";
import { draw, type PillRect, type Theme, type WipRow } from "@/lib/metro/draw";
import { isOnStation, laneAt, rowAtY } from "@/lib/metro/geometry";
import { type Density, graphContentWidth, metricsFor, rowTop } from "@/lib/metro/metrics";
import { GRAPH_WIDTH_MIN, REFS_WIDTH, useGraphSettings } from "@/composables/useGraphSettings";
import { useTheme } from "@/composables/useTheme";
import { type FetchPage, RowCache } from "@/lib/metro/rowCache";

/** What was right-clicked in the graph. */
export type GraphContextTarget =
  | { kind: "ref"; ref: RefInfo; commit: GraphRow | undefined; clientX: number; clientY: number }
  | { kind: "commit"; commit: GraphRow; clientX: number; clientY: number }
  | { kind: "lane"; lane: Lane; clientX: number; clientY: number };

const props = withDefaults(
  defineProps<{
    meta: GraphMeta;
    fetchPage: FetchPage;
    density?: Density;
    selected?: number | null;
    /** Vertical scroll to restore when the graph is shown (in pixels). */
    initialScrollTop?: number;
    /** Lane drawn in full while the others fade. */
    highlightLane?: number | null;
    /** Rows matching the current search, tinted. */
    matches?: readonly number[];
    /**
     * Where to put the view when `meta` is replaced by a refreshed graph. Without it a new
     * graph starts at the top.
     */
    anchor?: { row: number; offset: number } | null;
    /** A working tree row pinned above the graph. */
    wip?: WipRow | null;
    /** The row of the commit HEAD points at, tinted in the graph. */
    headRow?: number | null;
    /** The initials of the author in the stations (the stations are then bigger). */
    initials?: boolean;
  }>(),
  {
    density: "compact",
    selected: null,
    initialScrollTop: 0,
    highlightLane: null,
    matches: () => [],
    anchor: null,
    wip: null,
    headRow: null,
    initials: true,
  },
);

const emit = defineEmits<{
  "update:selected": [row: number | null];
  "update:density": [density: Density];
  "update:scrollTop": [scrollTop: number];
  /** The commit at the top of the view and how far it is scrolled (to keep the place on refresh). */
  "update:topCommit": [top: { id: string; offset: number } | null];
  /** The pinned working tree row was clicked. */
  wipClick: [];
  /** Right click on a ref label, a commit row or a lane line (positions are window coordinates). */
  contextMenu: [target: GraphContextTarget];
  select: [row: GraphRow];
  /** Double click on a ref label (`ref` set) or on a commit row. */
  activate: [target: { commit: GraphRow; ref: RefInfo | null }];
  /** The `+N` label of a commit was clicked: the branches it hides (positions are window coordinates). */
  moreRefs: [
    target: {
      items: Array<{ label: string; ref: RefInfo }>;
      commit: GraphRow | undefined;
      clientX: number;
      clientY: number;
    },
  ];
  /** The first and last rows in view, to tell where the rest of the graph is. */
  range: [first: number, last: number];
  /** A lane line was clicked (position relative to this component); `null` for any other click. */
  laneClick: [target: { lane: Lane; x: number; y: number } | null];
  error: [message: string];
}>();

const scroller = ref<HTMLDivElement>();
const canvas = ref<HTMLCanvasElement>();
const viewW = ref(0);
const viewH = ref(0);
const hoverRow = ref<number | null>(null);
const tooltip = ref<{ row: GraphRow; x: number; y: number } | null>(null);

const graphSettings = useGraphSettings();
const { refsWidth, graphWidth, columns } = graphSettings;

/** Height of the header with the names of the columns. */
const HEADER_H = 24;
/** Room kept for the commit message when a column is resized. */
const MESSAGE_MIN_W = 200;

const metrics = computed(() => metricsFor(props.density, props.initials));
const matchSet = computed(() => new Set(props.matches));
/** Height taken by the pinned working tree row. */
const inset = computed(() => (props.wip ? metrics.value.rowH : 0));
const contentH = computed(() => props.meta.total * metrics.value.rowH + inset.value);
const graphContentW = computed(() => graphContentWidth(props.meta.maxCols, metrics.value));
/** Left edge of the lane area: the column of branch labels comes first. */
const graphX = computed(() =>
  Math.min(refsWidth.value, Math.max(REFS_WIDTH.min, viewW.value - MESSAGE_MIN_W - 120)),
);
// The lane area fits the graph, up to ~45% of the rest, and scrolls sideways on its own; the user
// can give it a width of their own.
const graphViewportW = computed(() => {
  const room = Math.max(GRAPH_WIDTH_MIN, viewW.value - graphX.value);
  if (graphWidth.value !== null) {
    return Math.max(GRAPH_WIDTH_MIN, Math.min(graphWidth.value, room - MESSAGE_MIN_W));
  }
  return Math.min(graphContentW.value, Math.max(120, Math.floor(room * 0.45)));
});
const spacerW = computed(
  () => viewW.value + Math.max(0, graphContentW.value - graphViewportW.value),
);
const textX = computed(() => graphX.value + graphViewportW.value + 12);

/** Whether a position (relative to the scroller) is over the lane area. */
const inGraph = (x: number) => x >= graphX.value && x < graphX.value + graphViewportW.value;

let scrollTop = 0;
let scrollLeft = 0;
let frame = 0;
let pendingSelect: number | null = null;
/** A refreshed graph is loading: keep showing the previous picture until its rows arrive. */
let reloading = false;
let reloadTimer: ReturnType<typeof setTimeout> | undefined;
let reportedTop: { id: string; offset: number } | null = null;
/** Ref labels as last drawn, for right-click hit testing. */
let lastPills: PillRect[] = [];

const cache = new RowCache({
  fetchPage: (cursor, limit) => props.fetchPage(cursor, limit),
  onUpdate: () => {
    if (pendingSelect !== null) {
      const row = cache.getRow(pendingSelect);
      if (row) {
        pendingSelect = null;
        emit("select", row);
      }
    }
    if (reloading) {
      const { first, last } = visibleRange();
      if (cache.has(first, last)) endReload();
    }
    reportTop();
    scheduleDraw();
  },
  onError: (error) => emit("error", String(error)),
});

const appTheme = useTheme();

function readTheme(): Theme {
  const root = document.documentElement;
  const style = getComputedStyle(root);
  const dark = root.classList.contains("dark");
  const read = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
  return {
    background: read("--background", dark ? "#0d0d0d" : "#ffffff"),
    foreground: read("--foreground", dark ? "#fafafa" : "#171717"),
    muted: read("--muted-foreground", dark ? "#a3a3a3" : "#737373"),
    palette: appTheme.palette.value,
  };
}
let theme = readTheme();

function visibleRange() {
  const m = metrics.value;
  const first = Math.max(0, Math.floor(scrollTop / m.rowH));
  const last = Math.min(
    props.meta.total - 1,
    Math.floor((scrollTop + viewH.value - inset.value) / m.rowH),
  );
  return { first, last };
}

function loadVisible() {
  const { first, last } = visibleRange();
  cache.ensure(first, last, props.meta.total);
}

function endReload() {
  reloading = false;
  clearTimeout(reloadTimer);
  scheduleDraw();
}

/** Tells the parent which commit is at the top of the view, when it is known. */
function reportTop() {
  const row = Math.floor(scrollTop / metrics.value.rowH);
  const data = cache.getRow(row);
  if (!data) return;
  const offset = scrollTop - row * metrics.value.rowH;
  if (reportedTop?.id === data.id && reportedTop.offset === offset) return;
  reportedTop = { id: data.id, offset };
  emit("update:topCommit", reportedTop);
}

let reportedRange = "";
/** Tells the parent which rows are in view (so it can say where HEAD is). */
function reportRange() {
  const total = props.meta.total;
  if (total === 0 || viewH.value === 0) return;
  const { first, last } = visibleRange();
  const key = `${first}:${last}`;
  if (key === reportedRange) return;
  reportedRange = key;
  emit("range", first, last);
}

function scheduleDraw() {
  if (frame || reloading) return;
  frame = requestAnimationFrame(() => {
    frame = 0;
    render();
  });
}

function render() {
  const el = canvas.value;
  const ctx = el?.getContext("2d");
  if (!el || !ctx || viewW.value === 0 || viewH.value === 0) return;
  const dpr = window.devicePixelRatio || 1;
  if (el.width !== Math.round(viewW.value * dpr) || el.height !== Math.round(viewH.value * dpr)) {
    el.width = Math.round(viewW.value * dpr);
    el.height = Math.round(viewH.value * dpr);
    el.style.width = `${viewW.value}px`;
    el.style.height = `${viewH.value}px`;
  }
  const pills: PillRect[] = [];
  draw({
    ctx,
    width: viewW.value,
    height: viewH.value,
    dpr,
    scrollTop,
    graphScrollX: scrollLeft,
    graphViewportW: graphViewportW.value,
    graphX: graphX.value,
    textX: textX.value,
    columns: columns.value,
    remoteIcon: graphSettings.remoteIcon,
    meta: props.meta,
    metrics: metrics.value,
    density: props.density,
    theme,
    getRow: (row) => cache.getRow(row),
    selectedRow: props.selected,
    hoverRow: hoverRow.value,
    headRow: props.headRow,
    initials: props.initials,
    highlightLane: props.highlightLane,
    matchRows: matchSet.value,
    topInset: inset.value,
    wip: props.wip,
    pills,
  });
  lastPills = pills;
}

function onScroll() {
  const el = scroller.value;
  if (!el) return;
  scrollTop = el.scrollTop;
  scrollLeft = el.scrollLeft;
  emit("update:scrollTop", scrollTop);
  tooltip.value = null;
  reportTop();
  reportRange();
  loadVisible();
  scheduleDraw();
}

function pointerPosition(event: PointerEvent | MouseEvent) {
  const el = scroller.value;
  if (!el) return null;
  const rect = el.getBoundingClientRect();
  const x = event.clientX - rect.left;
  const y = event.clientY - rect.top;
  // Ignore the scrollbars.
  if (x < 0 || y < 0 || x >= el.clientWidth || y >= el.clientHeight) return null;
  return { x, y };
}

function onPointerMove(event: PointerEvent) {
  const point = pointerPosition(event);
  if (!point) return onPointerLeave();
  const m = metrics.value;
  // Positions in the scrolling content, below the pinned working tree row.
  const contentY = point.y - inset.value + scrollTop;
  const row = point.y < inset.value ? null : rowAtY(contentY, props.meta.total, m);
  let next: typeof tooltip.value = null;
  if (row !== null && inGraph(point.x)) {
    const data = cache.getRow(row);
    if (data && isOnStation(point.x - graphX.value + scrollLeft, contentY, row, data.col, m)) {
      next = {
        row: data,
        x: Math.min(point.x + 14, viewW.value - 300),
        y: point.y + 16 + HEADER_H,
      };
    }
  }
  tooltip.value = next;
  if (scroller.value) {
    const onLane =
      next === null &&
      inGraph(point.x) &&
      point.y >= inset.value &&
      laneAt(point.x - graphX.value + scrollLeft, contentY, props.meta.lanes, m) !== null;
    const onMore = lastPills.some(
      (p) =>
        p.more && point.x >= p.x && point.x <= p.x + p.w && point.y >= p.y && point.y <= p.y + p.h,
    );
    scroller.value.style.cursor = onLane || onMore ? "pointer" : "";
  }
  if (row !== hoverRow.value) {
    hoverRow.value = row;
    scheduleDraw();
  }
}

function onPointerLeave() {
  tooltip.value = null;
  if (hoverRow.value !== null) {
    hoverRow.value = null;
    scheduleDraw();
  }
}

function select(row: number | null) {
  emit("update:selected", row);
  if (row === null) return;
  const data = cache.getRow(row);
  if (data) {
    pendingSelect = null;
    emit("select", data);
  } else {
    pendingSelect = row;
  }
}

function onClick(event: MouseEvent) {
  const point = pointerPosition(event);
  if (!point) return;
  const m = metrics.value;
  if (point.y < inset.value) {
    emit("laneClick", null);
    emit("wipClick");
    return;
  }
  const contentY = point.y - inset.value + scrollTop;
  const row = rowAtY(contentY, props.meta.total, m);

  // The `+N` label lists the branches it hides.
  const more = lastPills.find(
    (p) =>
      p.more && point.x >= p.x && point.x <= p.x + p.w && point.y >= p.y && point.y <= p.y + p.h,
  );
  if (more?.more) {
    emit("laneClick", null);
    emit("moreRefs", {
      items: more.more,
      commit: cache.getRow(more.row),
      clientX: event.clientX,
      clientY: event.clientY,
    });
    return;
  }

  // A click on a lane line (not on a station) offers actions on that lane.
  if (inGraph(point.x)) {
    const data = row === null ? undefined : cache.getRow(row);
    const laneX = point.x - graphX.value + scrollLeft;
    const onStation =
      row !== null && data !== undefined && isOnStation(laneX, contentY, row, data.col, m);
    const lane = onStation ? null : laneAt(laneX, contentY, props.meta.lanes, m);
    if (lane) {
      emit("laneClick", { lane, x: point.x, y: point.y + HEADER_H });
      return;
    }
  }
  emit("laneClick", null);
  if (row !== null) select(row);
}

function onContextMenu(event: MouseEvent) {
  const point = pointerPosition(event);
  if (!point || point.y < inset.value) return;
  const m = metrics.value;
  const contentY = point.y - inset.value + scrollTop;
  const row = rowAtY(contentY, props.meta.total, m);
  const where = { clientX: event.clientX, clientY: event.clientY };
  emit("laneClick", null);

  // A ref label first: its menu is about the branch or tag.
  const pill = lastPills.find(
    (p) =>
      !p.more && point.x >= p.x && point.x <= p.x + p.w && point.y >= p.y && point.y <= p.y + p.h,
  );
  if (pill) {
    emit("contextMenu", { kind: "ref", ref: pill.ref, commit: cache.getRow(pill.row), ...where });
    return;
  }
  const data = row === null ? undefined : cache.getRow(row);
  if (inGraph(point.x)) {
    const laneX = point.x - graphX.value + scrollLeft;
    const onStation =
      row !== null && data !== undefined && isOnStation(laneX, contentY, row, data.col, m);
    const lane = onStation ? null : laneAt(laneX, contentY, props.meta.lanes, m);
    if (lane) {
      emit("contextMenu", { kind: "lane", lane, ...where });
      return;
    }
  }
  if (row !== null && data) {
    select(row);
    emit("contextMenu", { kind: "commit", commit: data, ...where });
  }
}

function onDoubleClick(event: MouseEvent) {
  const point = pointerPosition(event);
  if (!point || point.y < inset.value) return;
  const pill = lastPills.find(
    (p) =>
      !p.more && point.x >= p.x && point.x <= p.x + p.w && point.y >= p.y && point.y <= p.y + p.h,
  );
  if (pill) {
    const commit = cache.getRow(pill.row);
    if (commit) emit("activate", { commit, ref: pill.ref });
    return;
  }
  const row = rowAtY(point.y - inset.value + scrollTop, props.meta.total, metrics.value);
  const commit = row === null ? undefined : cache.getRow(row);
  if (commit) emit("activate", { commit, ref: null });
}

function scrollToRow(row: number) {
  const el = scroller.value;
  if (!el) return;
  const m = metrics.value;
  const top = rowTop(row, m);
  if (top < el.scrollTop) el.scrollTop = top;
  else if (top + m.rowH > el.scrollTop + el.clientHeight - inset.value) {
    el.scrollTop = top + m.rowH - (el.clientHeight - inset.value);
  }
}

/** Selects a row and scrolls it to the middle of the view. */
function focusRow(row: number) {
  const el = scroller.value;
  if (!el || row < 0 || row >= props.meta.total) return;
  const m = metrics.value;
  el.scrollTop = Math.max(0, rowTop(row, m) - (el.clientHeight - inset.value - m.rowH) / 2);
  select(row);
}

function onKeydown(event: KeyboardEvent) {
  const total = props.meta.total;
  if (total === 0) return;
  const page = Math.max(1, Math.floor((viewH.value - inset.value) / metrics.value.rowH) - 1);
  const current = props.selected ?? -1;
  const target: Record<string, number> = {
    ArrowDown: current + 1,
    ArrowUp: current - 1,
    PageDown: current + page,
    PageUp: current - page,
    Home: 0,
    End: total - 1,
  };
  if (!(event.key in target)) return;
  event.preventDefault();
  const row = Math.min(total - 1, Math.max(0, target[event.key]));
  select(row);
  scrollToRow(row);
}

let densityGestureAt = 0;
function onWheel(event: WheelEvent) {
  // Ctrl/Cmd + wheel (or trackpad pinch) switches the row density, once per gesture.
  if (!event.ctrlKey && !event.metaKey) return;
  event.preventDefault();
  const now = performance.now();
  if (now - densityGestureAt < 350) return;
  densityGestureAt = now;
  const next: Density = event.deltaY < 0 ? "detailed" : "compact";
  if (next !== props.density) emit("update:density", next);
}

let resizeObserver: ResizeObserver | undefined;
let themeObserver: MutationObserver | undefined;

onMounted(() => {
  const el = scroller.value;
  if (!el) return;
  resizeObserver = new ResizeObserver(() => {
    viewW.value = el.clientWidth;
    viewH.value = el.clientHeight;
    reportRange();
    loadVisible();
    scheduleDraw();
  });
  resizeObserver.observe(el);
  if (props.initialScrollTop > 0) {
    el.scrollTop = props.initialScrollTop;
    scrollTop = el.scrollTop;
  }
  themeObserver = new MutationObserver(() => {
    theme = readTheme();
    scheduleDraw();
  });
  themeObserver.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ["class", "style"],
  });
});

onBeforeUnmount(() => {
  resizeObserver?.disconnect();
  themeObserver?.disconnect();
  if (frame) cancelAnimationFrame(frame);
  clearTimeout(reloadTimer);
});

// A new graph: forget loaded rows and go back to the top.
watch(
  () => props.meta,
  async () => {
    const anchor = props.anchor;
    cache.reset();
    pendingSelect = null;
    hoverRow.value = null;
    tooltip.value = null;
    reportedTop = null;
    reportedRange = "";
    if (anchor) {
      // A refresh: stay where we were, and keep the old picture until the new rows arrive.
      reloading = true;
      clearTimeout(reloadTimer);
      reloadTimer = setTimeout(endReload, 1500);
      await nextTick();
      if (scroller.value) {
        scroller.value.scrollTop = anchor.row * metrics.value.rowH + anchor.offset;
        scrollTop = scroller.value.scrollTop;
      }
      loadVisible();
      reportRange();
      return;
    }
    await nextTick();
    if (scroller.value) {
      scroller.value.scrollTop = 0;
      scroller.value.scrollLeft = 0;
    }
    scrollTop = 0;
    scrollLeft = 0;
    loadVisible();
    reportRange();
    scheduleDraw();
  },
);

// Density (or size of the stations) change: keep the same commit at the top of the view.
watch(
  () => [props.density, props.initials] as const,
  async (_, [previousDensity, previousInitials]) => {
    const topRow = scrollTop / metricsFor(previousDensity, previousInitials).rowH;
    await nextTick();
    if (scroller.value) scroller.value.scrollTop = topRow * metrics.value.rowH;
    scrollTop = scroller.value?.scrollTop ?? 0;
    loadVisible();
    scheduleDraw();
  },
);

watch(
  () => [
    props.selected,
    props.highlightLane,
    props.matches,
    props.wip,
    graphViewportW.value,
    graphX.value,
    textX.value,
    columns.value,
    graphSettings.remoteIcons.value,
  ],
  scheduleDraw,
);

type Handle = "refs" | "graph";

/** Drags the divider between two columns. */
function startResize(handle: Handle, event: PointerEvent) {
  const target = event.currentTarget as HTMLElement;
  target.setPointerCapture(event.pointerId);
  const startX = event.clientX;
  const startRefs = graphX.value;
  const startGraph = graphViewportW.value;
  const move = (e: PointerEvent) => {
    const delta = e.clientX - startX;
    if (handle === "refs") {
      graphSettings.setRefsWidth(
        Math.min(startRefs + delta, viewW.value - MESSAGE_MIN_W - GRAPH_WIDTH_MIN),
      );
    } else {
      graphSettings.setGraphWidth(startGraph + delta);
    }
  };
  const stop = () => {
    target.removeEventListener("pointermove", move);
    target.removeEventListener("pointerup", stop);
    target.removeEventListener("pointercancel", stop);
  };
  target.addEventListener("pointermove", move);
  target.addEventListener("pointerup", stop);
  target.addEventListener("pointercancel", stop);
}

function resizeByKey(handle: Handle, event: KeyboardEvent) {
  const step = event.key === "ArrowLeft" ? -10 : event.key === "ArrowRight" ? 10 : 0;
  if (step === 0) return;
  event.preventDefault();
  if (handle === "refs") graphSettings.setRefsWidth(graphX.value + step);
  else graphSettings.setGraphWidth(graphViewportW.value + step);
}

/** Double click: the lane area fits the graph again (the labels go back to their usual width). */
function resetResize(handle: Handle) {
  if (handle === "refs") graphSettings.setRefsWidth(REFS_WIDTH.initial);
  else graphSettings.setGraphWidth(null);
}

defineExpose({ scrollToRow, focusRow });

const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });
</script>

<template>
  <div class="relative flex h-full w-full flex-col overflow-hidden">
    <div
      class="text-muted-foreground bg-background relative shrink-0 border-b text-[10px] font-medium tracking-wide uppercase select-none"
      :style="{ height: `${HEADER_H}px` }"
    >
      <span class="absolute top-0 flex h-full items-center pl-2" :style="{ left: '0px' }">
        Branch / tag
      </span>
      <span class="absolute top-0 flex h-full items-center pl-2" :style="{ left: `${graphX}px` }">
        Graph
      </span>
      <span
        class="absolute top-0 flex h-full items-center pl-2"
        :style="{ left: `${textX - 12}px` }"
      >
        Commit message
      </span>
      <div
        v-for="handle in ['refs', 'graph'] as const"
        :key="handle"
        class="hover:bg-ring/50 focus-visible:bg-ring/50 absolute top-0 z-10 h-full w-1.5 -translate-x-1/2 cursor-col-resize"
        :style="{ left: `${handle === 'refs' ? graphX : graphX + graphViewportW}px` }"
        role="separator"
        aria-orientation="vertical"
        :aria-label="handle === 'refs' ? 'Resize the branch column' : 'Resize the graph column'"
        tabindex="0"
        @pointerdown.prevent="startResize(handle, $event)"
        @keydown="resizeByKey(handle, $event)"
        @dblclick="resetResize(handle)"
      />
    </div>
    <div
      ref="scroller"
      class="min-h-0 w-full flex-1 overflow-auto outline-none"
      tabindex="0"
      role="listbox"
      aria-label="Commit graph"
      @scroll.passive="onScroll"
      @pointermove="onPointerMove"
      @pointerleave="onPointerLeave"
      @click="onClick"
      @dblclick="onDoubleClick"
      @contextmenu.prevent="onContextMenu"
      @keydown="onKeydown"
      @wheel="onWheel"
    >
      <div class="sticky top-0 left-0 h-0 w-0">
        <canvas ref="canvas" class="absolute top-0 left-0 block" />
      </div>
      <div :style="{ height: `${contentH}px`, width: `${spacerW}px` }" />
    </div>

    <div
      v-if="tooltip"
      class="bg-popover text-popover-foreground pointer-events-none absolute z-10 w-72 rounded-md border p-2.5 text-xs shadow-md"
      :style="{ left: `${tooltip.x}px`, top: `${tooltip.y}px` }"
    >
      <p class="mb-1 text-sm leading-snug font-medium break-words">{{ tooltip.row.summary }}</p>
      <p class="text-muted-foreground font-mono">{{ tooltip.row.id.slice(0, 10) }}</p>
      <p class="text-muted-foreground">
        {{ tooltip.row.author }} · {{ dateFormat.format(new Date(tooltip.row.time * 1000)) }}
      </p>
      <p v-if="tooltip.row.parents.length > 1" class="text-muted-foreground">
        Merge of {{ tooltip.row.parents.length }} parents
      </p>
    </div>
  </div>
</template>
