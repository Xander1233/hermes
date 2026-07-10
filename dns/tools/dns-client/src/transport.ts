import dgram from "node:dgram";
import net from "node:net";

export type Transport = "udp" | "tcp";

/** Sends a query and resolves with the raw response bytes. */
export async function sendQuery(
  transport: Transport,
  server: string,
  port: number,
  query: Buffer,
  timeoutMs: number,
): Promise<Buffer> {
  return transport === "tcp"
    ? sendTcp(server, port, query, timeoutMs)
    : sendUdp(server, port, query, timeoutMs);
}

function sendUdp(server: string, port: number, query: Buffer, timeoutMs: number): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    const socket = dgram.createSocket(server.includes(":") ? "udp6" : "udp4");
    const timer = setTimeout(() => {
      socket.close();
      reject(new Error(`timed out after ${timeoutMs} ms waiting for a UDP response`));
    }, timeoutMs);

    socket.once("message", (msg) => {
      clearTimeout(timer);
      socket.close();
      resolve(msg);
    });
    socket.once("error", (err) => {
      clearTimeout(timer);
      socket.close();
      reject(err);
    });

    socket.send(query, port, server, (err) => {
      if (err) {
        clearTimeout(timer);
        socket.close();
        reject(err);
      }
    });
  });
}

function sendTcp(server: string, port: number, query: Buffer, timeoutMs: number): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    // DNS over TCP frames each message with a 2-byte big-endian length prefix.
    const framed = Buffer.alloc(2 + query.length);
    framed.writeUInt16BE(query.length, 0);
    query.copy(framed, 2);

    const socket = net.connect({ host: server, port });
    socket.setTimeout(timeoutMs);

    let received = Buffer.alloc(0);
    let expected = -1;

    socket.on("connect", () => socket.write(framed));
    socket.on("data", (chunk) => {
      received = Buffer.concat([received, chunk]);
      if (expected < 0 && received.length >= 2) {
        expected = received.readUInt16BE(0);
        received = received.subarray(2);
      }
      if (expected >= 0 && received.length >= expected) {
        socket.end();
        resolve(received.subarray(0, expected));
      }
    });
    socket.on("timeout", () => {
      socket.destroy();
      reject(new Error(`timed out after ${timeoutMs} ms waiting for a TCP response`));
    });
    socket.on("error", reject);
  });
}
