// Asks the operating system for a loopback port nothing is using.
//
// Browser verification used to require 127.0.0.1:3000 and :3001 to be free. On a
// machine shared with other work that is not a property anyone controls, and the
// only remedy on offer was to find and stop whatever held the port - someone
// else's process, on someone else's project. Allocating instead means the smoke
// coexists rather than competes (#35).
//
// The port is obtained by binding it and letting go, so between here and the
// real bind there is a window in which something else could take it. That window
// is small and the failure is loud (the server refuses to start on a port it
// cannot bind), which is a better trade than a fixed port that is unavailable by
// design. `allocatePorts` never hands back the same number twice in one call.

import net from "node:net";

/**
 * One free loopback port, chosen by the OS.
 *
 * @param {{ host?: string }} [options]
 * @returns {Promise<number>}
 */
export function allocatePort({ host = "127.0.0.1" } = {}) {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.unref();
    server.on("error", reject);
    server.listen({ host, port: 0 }, () => {
      const address = server.address();
      if (!address || typeof address === "string") {
        server.close(() => reject(new Error("The OS did not report a bound port.")));
        return;
      }
      const { port } = address;
      server.close((error) => (error ? reject(error) : resolve(port)));
    });
  });
}

/**
 * `count` distinct free loopback ports.
 *
 * Allocated one at a time while holding the earlier sockets open, so the OS
 * cannot hand out the same port twice within a single call.
 *
 * @param {number} count
 * @param {{ host?: string }} [options]
 * @returns {Promise<number[]>}
 */
export async function allocatePorts(count, { host = "127.0.0.1" } = {}) {
  if (!Number.isInteger(count) || count < 1) {
    throw new Error(`count must be a positive integer, got ${count}`);
  }

  const held = [];
  try {
    for (let index = 0; index < count; index += 1) {
      held.push(await holdPort(host));
    }
    return held.map(({ port }) => port);
  } finally {
    await Promise.all(held.map(({ release }) => release()));
  }
}

function holdPort(host) {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.unref();
    server.on("error", reject);
    server.listen({ host, port: 0 }, () => {
      const address = server.address();
      if (!address || typeof address === "string") {
        server.close(() => reject(new Error("The OS did not report a bound port.")));
        return;
      }
      resolve({
        port: address.port,
        release: () => new Promise((done) => server.close(() => done())),
      });
    });
  });
}
