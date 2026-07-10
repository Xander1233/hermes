import { reactRouter } from "@react-router/dev/vite";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

export default defineConfig({
  resolve: { tsconfigPaths: true },
  plugins: [tailwindcss(), reactRouter()],

  // `amazon-cognito-identity-js` pulls in the `buffer` shim, which expects Node's `global`.
  // Without this the dependency optimizer throws `ReferenceError: global is not defined` and the
  // whole route renders blank. Browsers spell it `globalThis`.
  define: { global: "globalThis" },
});
