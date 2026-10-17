FROM node:22-alpine AS development-dependencies-env
COPY . /app
WORKDIR /app
RUN npm ci

FROM node:22-alpine AS production-dependencies-env
COPY ./package.json package-lock.json /app/
WORKDIR /app
RUN npm ci --omit=dev

FROM node:22-alpine AS build-env
COPY . /app/
COPY --from=development-dependencies-env /app/node_modules /app/node_modules
WORKDIR /app

# Every VITE_* value is inlined into the client bundle by `npm run build`, so it has to be known
# here rather than at `docker run` time — hence build args instead of runtime environment. Vite
# picks prefixed variables up from the process environment, which is what the ENV lines are for.
#
# An arg left unset becomes an empty string, and app/lib/config.ts treats that as absent and uses
# its own fallback (mock API, localhost URLs). So an unconfigured build still works; it just does
# not talk to a real backend. Nothing secret belongs here: these end up in the shipped JS.
ARG VITE_USE_MOCK_API
ARG VITE_API_BASE_URL
ARG VITE_COGNITO_REGION
ARG VITE_COGNITO_USER_POOL_ID
ARG VITE_COGNITO_CLIENT_ID
ARG VITE_COGNITO_DOMAIN
ARG VITE_COGNITO_REDIRECT_URI
ARG VITE_COGNITO_LOGOUT_URI
ARG VITE_COGNITO_SCOPES

ENV VITE_USE_MOCK_API=$VITE_USE_MOCK_API \
    VITE_API_BASE_URL=$VITE_API_BASE_URL \
    VITE_COGNITO_REGION=$VITE_COGNITO_REGION \
    VITE_COGNITO_USER_POOL_ID=$VITE_COGNITO_USER_POOL_ID \
    VITE_COGNITO_CLIENT_ID=$VITE_COGNITO_CLIENT_ID \
    VITE_COGNITO_DOMAIN=$VITE_COGNITO_DOMAIN \
    VITE_COGNITO_REDIRECT_URI=$VITE_COGNITO_REDIRECT_URI \
    VITE_COGNITO_LOGOUT_URI=$VITE_COGNITO_LOGOUT_URI \
    VITE_COGNITO_SCOPES=$VITE_COGNITO_SCOPES

RUN npm run build

FROM node:22-alpine
COPY ./package.json package-lock.json /app/
COPY --from=production-dependencies-env /app/node_modules /app/node_modules
COPY --from=build-env /app/build /app/build
WORKDIR /app
CMD ["npm", "run", "start"]
