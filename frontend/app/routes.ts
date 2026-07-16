import {
  type RouteConfig,
  index,
  layout,
  route,
} from "@react-router/dev/routes";

export default [
  // Public routes
  route("login", "routes/login.tsx"),
  route("auth/callback", "routes/auth-callback.tsx"),

  // Protected routes
  layout("routes/app-layout.tsx", [
    index("routes/zones.tsx"),

    route("profile", "routes/profile-layout.tsx", [
      index("routes/profile.details.tsx"),
      route("security", "routes/profile.security.tsx"),
    ]),

    route("zones/:zoneId", "routes/zone-layout.tsx", [
      index("routes/zone.records.tsx"),
      route("soa", "routes/zone.soa.tsx"),
      route("nameservers", "routes/zone.nameservers.tsx"),
    ]),
  ]),
] satisfies RouteConfig;
