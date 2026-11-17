// src/ipc は @tauri-apps/api を import してよい唯一の場所。
import { invoke } from "@tauri-apps/api/core";
import type { Event } from "@tauri-apps/api/event";

export async function ping(): Promise<string> {
  return invoke<string>("ping");
}

export type AppEvent = Event<string>;
