import {
  Links,
  Meta,
  Outlet,
  Scripts,
  ScrollRestoration,
  isRouteErrorResponse,
} from "react-router";
import { RiArrowLeftLine } from "@remixicon/react";

import type { Route } from "./+types/root";
import { Button } from "~/components/ui/button";
import { Toaster } from "~/components/ui/sonner";
import { TooltipProvider } from "~/components/ui/tooltip";
import "./app.css";

/**
 * Restores the stored theme and sidebar width before first paint.
 *
 * Runs blocking in <head> on purpose. Reading these after hydration means the
 * page renders light and snaps to dark, and the rail renders wide and snaps
 * narrow — a flash and a layout shift, both small and both completely
 * avoidable. Both are expressed as attributes on <html> so CSS resolves them in
 * the first frame and React never has to own the visual state.
 */
const BOOT_SCRIPT = `(function(){try{
var d=document.documentElement;
var s=localStorage.getItem('hermes-theme');
var dark=s==='dark'||(!s&&window.matchMedia('(prefers-color-scheme: dark)').matches);
d.classList.toggle('dark',dark);
d.style.colorScheme=dark?'dark':'light';
if(localStorage.getItem('hermes-sidebar')==='collapsed'){d.setAttribute('data-sidebar','collapsed');}
}catch(e){}})()`;

export function Layout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" suppressHydrationWarning>
      <head>
        <meta charSet="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="color-scheme" content="light dark" />
        <Meta />
        <Links />
        <script dangerouslySetInnerHTML={{ __html: BOOT_SCRIPT }} />
      </head>
      <body>
        <TooltipProvider>{children}</TooltipProvider>
        <Toaster position="bottom-right" />
        <ScrollRestoration />
        <Scripts />
      </body>
    </html>
  );
}

export default function App() {
  return <Outlet />;
}

export function ErrorBoundary({ error }: Route.ErrorBoundaryProps) {
  let title = "Something went wrong";
  let detail = "An unexpected error occurred. Reloading may be enough.";
  let stack: string | undefined;
  let showContactSupport = true;

  if (isRouteErrorResponse(error)) {
    switch (error.status) {
      case 404:
        title = "Page not found";
        detail = "That URL doesn't match anything here. It may have been a zone that was deleted.";
        showContactSupport = true;
        break;

      case 401:
        title = "Session expired";
        detail = "Your sign-in is no longer valid. Sign in again to continue.";
        showContactSupport = false;
        break;

      default:
        title = `Error ${error.status}`;
        detail = error.statusText || detail;
        break;
    };
  } else if (error instanceof Error) {
    if (import.meta.env.DEV) {
      detail = error.message;
      stack = error.stack;
    } else {
      detail = "An unexpected error occurred. Reloading may be enough. If the problem persists, please contact support.";
    }
  }

  return (
    <main className="mx-auto flex min-h-svh w-full max-w-2xl flex-col justify-center gap-6 px-6 py-16">
      <div className="space-y-2">
        <h1 className="text-2xl font-medium tracking-tight">{title}</h1>
        <p className="max-w-[60ch] text-muted-foreground">{detail}</p>
      </div>

      <div>
        <Button variant="outline" size="sm" render={<a href="/" />}>
          <RiArrowLeftLine />
          Back to zones
        </Button>
      </div>

      {stack && (
        <pre className="max-h-80 overflow-auto rounded-lg border bg-surface-2 p-4 text-xs">
          <code className="font-data">{stack}</code>
        </pre>
      )}

      {showContactSupport && (
        <p className="text-sm text-muted-foreground">
          If the problem persists, please{" "}
          <a
            href="mailto:contact@david-neidhart.de"
            className="font-medium underline underline-offset-4"
          >
            contact support
          </a>
          .
        </p>
      )}
    </main>
  );
}
