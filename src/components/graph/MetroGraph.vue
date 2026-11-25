<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { GraphMeta, GraphRow, Lane } from "@/lib/api";
import { draw, type Theme, type WipRow } from "@/lib/metro/draw";
import { isOnStation, laneAt, rowAtY } from "@/lib/metro/geometry";
import { type Density, graphContentWidth, METRICS, rowTop } from "@/lib/metro/metrics";
import { DARK_PALETTE, LIGHT_PALETTE } from "@/lib/metro/palette";
import { type FetchPage, RowCache } from "@/lib/metro/rowCache";

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
  }>(),
  {
    density: "compact",
    selected: null,
    initialScrollTop: 0,
    highlightLane: null,
    matches: () => [],
    anchor: null,
    wip: null,
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
  select: [row: GraphRow];
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

const metrics = computed(() => METRICS[props.density]);
const matchSet = computed(() => new Set(props.matches));
/** Height taken by the pinned working tree row. */
const inset = computed(() => (props.wip ? metrics.value.rowH : 0));
const contentH = computed(() => props.meta.total * metrics.value.rowH + inset.value);
const graphContentW = computed(() => graphContentWidth(props.meta.maxCols, metrics.value));
// The lane area takes at most ~45% of the width and scrolls sideways on its own.
const graphViewportW = computed(() =>
  Math.min(graphContentW.value, Math.max(120, Math.floor(viewW.value * 0.45))),
);
const spacerW = computed(
  () => viewW.value + Math.max(0, graphContentW.value - graphViewportW.value),
);
const textX = computed(() => graphViewportW.value + 12);

let scrollTop = 0;
let scrollLeft = 0;
let frame = 0;
let pendingSelect: number | null = null;
/** A refreshed graph is loading: keep showing the previous picture until its rows arrive. */
let reloading = false;
let reloadTimer: ReturnType<typeof setTimeout> | undefined;
let reportedTop: { id: string; offset: number } | null = null;

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

function readTheme(): Theme {
  const root = document.documentElement;
  const style = getComputedStyle(root);
  const dark = root.classList.contains("dark");
  const read = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
  return {
    background: read("--background", dark ? "#0d0d0d" : "#ffffff"),
    foreground: read("--foreground", dark ? "#fafafa" : "#171717"),
    muted: read("--muted-foreground", dark ? "#a3a3a3" : "#737373"),
    palette: dark ? DARK_PALETTE : LIGHT_PALETTE,
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
  draw({
    ctx,
    width: viewW.value,
    height: viewH.value,
    dpr,
    scrollTop,
    graphScrollX: scrollLeft,
    graphViewportW: graphViewportW.value,
    textX: textX.value,
    meta: props.meta,
    metrics: metrics.value,
    density: props.density,
    theme,
    getRow: (row) => cache.getRow(row),
    selectedRow: props.selected,
    hoverRow: hoverRow.value,
    highlightLane: props.highlightLane,
    matchRows: matchSet.value,
    topInset: inset.value,
    wip: props.wip,
  });
}

function onScroll() {
  const el = scroller.value;
  if (!el) return;
  scrollTop = el.scrollTop;
  scrollLeft = el.scrollLeft;
  emit("update:scrollTop", scrollTop);
  tooltip.value = null;
  reportTop();
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
  if (row !== null && point.x < graphViewportW.value) {
    const data = cache.getRow(row);
    if (data && isOnStation(point.x + scrollLeft, contentY, row, data.col, m)) {
      next = { row: data, x: Math.min(point.x + 14, viewW.value - 300), y: point.y + 16 };
    }
  }
  tooltip.value = next;
  if (scroller.value) {
    const onLane =
      next === null &&
      point.x < graphViewportW.value &&
      point.y >= inset.value &&
      laneAt(point.x + scrollLeft, contentY, props.meta.lanes, m) !== null;
    scroller.value.style.cursor = onLane ? "pointer" : "";
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

  // A click on a lane line (not on a station) offers actions on that lane.
  if (point.x < graphViewportW.value) {
    const data = row === null ? undefined : cache.getRow(row);
    const onStation =
      row !== null &&
      data !== undefined &&
      isOnStation(point.x + scrollLeft, contentY, row, data.col, m);
    const lane = onStation ? null : laneAt(point.x + scrollLeft, contentY, props.meta.lanes, m);
    if (lane) {
      emit("laneClick", { lane, x: point.x, y: point.y });
      return;
    }
  }
  emit("laneClick", null);
  if (row !== null) select(row);
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
  themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] });
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
    scheduleDraw();
  },
);

// Density change: keep the same commit at the top of the view.
watch(
  () => props.density,
  async (_, previous) => {
    const topRow = scrollTop / METRICS[previous].rowH;
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
    textX.value,
  ],
  scheduleDraw,
);

defineExpose({ scrollToRow, focusRow });

const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });
</script>

<template>
  <div class="relative h-full w-full overflow-hidden">
    <div
      ref="scroller"
      class="h-full w-full overflow-auto outline-none"
      tabindex="0"
      role="listbox"
      aria-label="Commit graph"
      @scroll.passive="onScroll"
      @pointermove="onPointerMove"
      @pointerleave="onPointerLeave"
      @click="onClick"
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
