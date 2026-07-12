import { config } from "~/lib/config";
import { httpApi } from "~/lib/api/http";
import { mockApi } from "~/lib/api/mock";
import type { HermesApi } from "~/lib/api/types";

// switch out normal http API with mock if applicable
export const api: HermesApi = config.useMockApi ? mockApi : httpApi;

export * from "~/lib/api/types";
