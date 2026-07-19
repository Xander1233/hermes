# Build stage ------------------------------------------------------------------------------
FROM mcr.microsoft.com/dotnet/sdk:10.0 AS build
WORKDIR /src

# Restore before copying the rest of the source, so a code-only change does not invalidate the
# restore layer. The csproj files are all that `dotnet restore` reads.
COPY Directory.Build.props ./
COPY Hermes.Mgmt.slnx ./
COPY src/Hermes.Mgmt.Common/*.csproj      src/Hermes.Mgmt.Common/
COPY src/Hermes.Mgmt.Repository/*.csproj  src/Hermes.Mgmt.Repository/
COPY src/Hermes.Mgmt.Service/*.csproj     src/Hermes.Mgmt.Service/
COPY src/Hermes.Mgmt.Api/*.csproj         src/Hermes.Mgmt.Api/
RUN dotnet restore src/Hermes.Mgmt.Api/Hermes.Mgmt.Api.csproj

COPY src/ src/
RUN dotnet publish src/Hermes.Mgmt.Api/Hermes.Mgmt.Api.csproj \
      -c Release \
      -o /app \
      --no-restore \
      /p:UseAppHost=false

# Runtime stage ----------------------------------------------------------------------------
# The ASP.NET runtime image, not the SDK: the compiler and NuGet cache have no business in a
# production image, and leaving them there roughly triples its size.
FROM mcr.microsoft.com/dotnet/aspnet:10.0 AS runtime
WORKDIR /app

# Run unprivileged. The image ships an `app` user (uid 1654) for exactly this.
USER app

COPY --from=build --chown=app:app /app .

# Kestrel binds this; the container listens on 8080 so it needs no privileged port.
ENV ASPNETCORE_URLS=http://+:8080 \
    ASPNETCORE_ENVIRONMENT=Production \
    DOTNET_gcServer=1
EXPOSE 8080

# Configuration comes from the environment, using the `__` separator for nested keys, e.g.
#   Cognito__UserPoolId, DynamoDb__TableName, Cache__Provider, ZoneSyncApi__Token.
# Do not bake secrets into the image; ZoneSyncApi__Token in particular grants read access to
# every tenant's zone data.
#
# AWS credentials should come from the task/pod role, not from environment variables — the SDK
# picks those up automatically.

# `/healthz` is anonymous and does not touch DynamoDB, so it stays a liveness probe rather than
# a dependency check.
ENTRYPOINT ["dotnet", "Hermes.Mgmt.Api.dll"]
