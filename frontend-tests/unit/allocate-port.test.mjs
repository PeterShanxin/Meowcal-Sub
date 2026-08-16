import net from "node:net";
import { describe, expect, it } from "vitest";
import { allocatePort, allocatePorts } from "../../scripts/allocate-port.mjs";

function isBindable(port) {
  return new Promise((resolve) => {
    const server = net.createServer();
    server.on("error", () => resolve(false));
    server.listen({ host: "127.0.0.1", port }, () => {
      server.close(() => resolve(true));
    });
  });
}

describe("allocating a loopback port", () => {
  it("returns a port in the usable range", async () => {
    const port = await allocatePort();
    expect(Number.isInteger(port)).toBe(true);
    expect(port).toBeGreaterThan(0);
    expect(port).toBeLessThanOrEqual(65_535);
  });

  it("returns a port that can then be bound", async () => {
    const port = await allocatePort();
    await expect(isBindable(port)).resolves.toBe(true);
  });

  it("does not hand out the same port twice in one call", async () => {
    const ports = await allocatePorts(4);
    expect(ports).toHaveLength(4);
    expect(new Set(ports).size).toBe(4);
  });

  it("releases every port it held while allocating", async () => {
    const ports = await allocatePorts(3);
    for (const port of ports) {
      await expect(isBindable(port)).resolves.toBe(true);
    }
  });

  it("refuses a nonsensical count", async () => {
    await expect(allocatePorts(0)).rejects.toThrow(/positive integer/);
    await expect(allocatePorts(-1)).rejects.toThrow(/positive integer/);
    await expect(allocatePorts(1.5)).rejects.toThrow(/positive integer/);
  });
});
