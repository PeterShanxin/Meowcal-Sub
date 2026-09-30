export async function connectNativePage(url, port = "9241") {
  const pages = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
  const page = pages.find((entry) => entry.url === url);
  if (!page) throw new Error(`Native WebView not found: ${url}`);
  const socket = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });
  let id = 0;
  const pending = new Map();
  socket.addEventListener("message", ({ data }) => {
    const reply = JSON.parse(data);
    const flight = pending.get(reply.id);
    if (!flight) return;
    pending.delete(reply.id);
    if (reply.error || reply.result.exceptionDetails)
      flight.reject(new Error(JSON.stringify(reply)));
    else flight.resolve(reply.result.result.value);
  });
  return {
    evaluate(expression) {
      return new Promise((resolve, reject) => {
        pending.set(++id, { resolve, reject });
        socket.send(
          JSON.stringify({
            id,
            method: "Runtime.evaluate",
            params: {
              expression,
              awaitPromise: true,
              returnByValue: true,
            },
          }),
        );
      });
    },
    close() {
      socket.close();
    },
  };
}
