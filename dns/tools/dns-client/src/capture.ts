// Debug helper: bind a UDP port and print every DNS packet received (raw + decoded),
// to inspect what a client such as nslookup actually sends. Does not reply.
import dgram from "node:dgram";
import { decodeMessage, typeName } from "./protocol.ts";

const port = Number(process.argv[2] ?? 5399);
const sock = dgram.createSocket("udp4");

sock.on("message", (msg, rinfo) => {
  console.log(`\n--- ${msg.length} bytes from ${rinfo.address}:${rinfo.port} ---`);
  console.log("hex:", msg.toString("hex"));
  try {
    const m = decodeMessage(msg);
    console.log(
      "header:",
      `id=${m.header.id} opcode=${m.header.opcode} rd=${m.header.rd}`,
      `qd=${m.header.qdcount} an=${m.header.ancount} ns=${m.header.nscount} ar=${m.header.arcount}`,
    );
    console.log("questions:", m.questions.map((q) => `${q.name} ${typeName(q.type)}`).join(", "));
    console.log(
      "additionals:",
      m.additionals.map((r) => `${r.name || "<root>"} ${typeName(r.type)} rdlen=${r.rdlength}`).join(", ") || "(none)",
    );
  } catch (e) {
    console.log("decode error:", (e as Error).message);
  }
});

sock.bind(port, "127.0.0.1", () => console.log(`capturing on 127.0.0.1:${port}`));
setTimeout(() => { sock.close(); process.exit(0); }, 9000);
