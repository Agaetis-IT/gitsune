import type { MenuNode } from "@/lib/appMenu";

/** The menu bar is the application's own on macOS only; elsewhere the window keeps its toolbar. */
export function hasNativeMenu(): boolean {
  return (
    typeof navigator !== "undefined" &&
    /Mac/i.test(navigator.userAgent) &&
    typeof window !== "undefined" &&
    "__TAURI_INTERNALS__" in window
  );
}

let building = false;
let queued: { nodes: MenuNode[]; onAction: (id: string) => void } | null = null;

/**
 * Replaces the menu bar of the application. Calls made while a menu is being built are folded
 * into one: only the latest state is built next.
 */
export async function setNativeMenu(
  nodes: MenuNode[],
  onAction: (id: string) => void,
): Promise<void> {
  queued = { nodes, onAction };
  if (building) return;
  building = true;
  try {
    while (queued) {
      const next = queued;
      queued = null;
      await build(next.nodes, next.onAction);
    }
  } finally {
    building = false;
  }
}

async function build(nodes: MenuNode[], onAction: (id: string) => void): Promise<void> {
  const { CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu } =
    await import("@tauri-apps/api/menu");

  async function make(node: MenuNode): Promise<Awaited<ReturnType<typeof PredefinedMenuItem.new>>> {
    if (node.kind === "separator") return PredefinedMenuItem.new({ item: "Separator" });
    if (node.kind === "predefined") {
      return PredefinedMenuItem.new({
        item: node.item === "About" ? { About: null } : node.item,
      });
    }
    if (node.kind === "submenu") {
      const items = await Promise.all(node.items.map(make));
      return Submenu.new({ text: node.label, items, enabled: node.enabled ?? true }) as never;
    }
    const options = {
      id: node.id,
      text: node.label,
      enabled: node.enabled,
      action: () => onAction(node.id),
    };
    const create = (accelerator?: string) =>
      node.checked === undefined
        ? MenuItem.new({ ...options, accelerator })
        : CheckMenuItem.new({ ...options, accelerator, checked: node.checked });
    try {
      return (await create(node.accelerator)) as never;
    } catch {
      // A shortcut the system does not accept must not take the entry with it.
      return (await create(undefined)) as never;
    }
  }

  const items = await Promise.all(nodes.map(make));
  const menu = await Menu.new({ items });
  await menu.setAsAppMenu();
}
