export async function sync(): Promise<void> {
  await window.fetch ("https://example.com");
  const xhr = new XMLHttpRequest();
  const ws = new WebSocket("wss://example.com");
  const es = new EventSource("/events");
  navigator.sendBeacon("/beacon", "x");
  void xhr;
  void ws;
  void es;
}
