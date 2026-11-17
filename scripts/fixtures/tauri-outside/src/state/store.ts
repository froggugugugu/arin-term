import {
  invoke,
} from "@tauri-apps/api/core";

export async function load(): Promise<unknown> {
  const { listen } = await import("@tauri-apps/api/event");
  void listen;
  return invoke("load");
}
