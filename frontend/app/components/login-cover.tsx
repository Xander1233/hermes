import { useEffect, useRef } from "react";

const ORIGIN_MIN_X = 10;
const ORIGIN_MAX_X = 30;

const ORIGIN_MIN_Y = 70;
const ORIGIN_MAX_Y = 90;

const RING_GAP = 68;

export function LoginCover() {
  const isRendered = useRef(false);
  const origin = useRef("18% 78%");

  if (!isRendered.current) {
    const x = Math.floor(Math.random() * (ORIGIN_MAX_X - ORIGIN_MIN_X + 1) + ORIGIN_MIN_X);
    const y = Math.floor(Math.random() * (ORIGIN_MAX_Y - ORIGIN_MIN_Y + 1) + ORIGIN_MIN_Y);
    origin.current = `${x}% ${y}%`;
  }

  useEffect(() => {
    isRendered.current = true;
  }, []);

  return (
    <div
      aria-hidden="true"
      className="relative hidden overflow-hidden border-r bg-surface lg:block"
    >
      <div
        className="absolute inset-0"
        style={{
          backgroundImage: [
            `repeating-radial-gradient(circle at ${origin.current}, color-mix(in oklch, var(--primary) 14%, transparent) 0 3px, transparent 1px ${RING_GAP}px)`,
            `radial-gradient(circle at ${origin.current}, color-mix(in oklch, var(--primary) 24%, transparent) 0, transparent 55%)`,
            `radial-gradient(circle at 85% 14%, color-mix(in oklch, var(--primary) 9%, transparent) 0%, transparent 45%)`,
          ].join(", "),
        }}
      />

      <div
        className="absolute inset-0"
        style={{
          backgroundImage:
            "linear-gradient(100deg, transparent 55%, color-mix(in oklch, var(--background) 85%, transparent) 100%)",
        }}
      />
    </div>
  );
}
