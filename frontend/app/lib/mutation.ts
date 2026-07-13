import { ApiError } from "~/lib/api";

export type ActionResult<TData = undefined> =
  | { ok: true; data: TData }
  | { ok: false; message: string; fieldErrors: Record<string, string[]> };

export async function runMutation<TData>(
  mutate: () => Promise<TData>
): Promise<ActionResult<TData>> {
  try {
    return { ok: true, data: await mutate() };
  } catch (cause) {
    
    if (cause instanceof ApiError) {
      return {
        ok: false,
        message: cause.message,
        fieldErrors: cause.fieldErrors,
      };
    }

    if (cause instanceof Error && cause.name === "AuthError") {
      return { ok: false, message: cause.message, fieldErrors: {} };
    }

    return {
      ok: false,
      message: "Something went wrong. Please try again.",
      fieldErrors: {},
    };
  }
}

export function firstFieldErrors(
  fieldErrors: Record<string, string[]>
): Record<string, string> {
  return Object.fromEntries(
    Object.entries(fieldErrors).map(([field, messages]) => [field, messages[0]])
  );
}
