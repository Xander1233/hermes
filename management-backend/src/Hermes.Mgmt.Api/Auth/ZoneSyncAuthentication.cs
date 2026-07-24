using System.Security.Claims;
using System.Text.Encodings.Web;
using Microsoft.AspNetCore.Authentication;
using Microsoft.Extensions.Options;

namespace Hermes.Mgmt.Api.Auth;

public static class AuthConstants
{
    public const string DashboardScheme = "Cognito";
    public const string ZoneSyncScheme = "ZoneSync";
    public const string DashboardPolicy = "Dashboard";
    public const string ZoneSyncPolicy = "ZoneSync";
}

public sealed class ZoneSyncAuthOptions : AuthenticationSchemeOptions
{
    public const string Section = "ZoneSyncApi";

    public string? Token { get; set; }
}

public sealed class ZoneSyncAuthenticationHandler(
    IOptionsMonitor<ZoneSyncAuthOptions> options,
    ILoggerFactory logger,
    UrlEncoder encoder) : AuthenticationHandler<ZoneSyncAuthOptions>(options, logger, encoder)
{
    protected override Task<AuthenticateResult> HandleAuthenticateAsync()
    {
        var expected = Options.Token;
        if (string.IsNullOrWhiteSpace(expected))
        {
            return Task.FromResult(AuthenticateResult.Fail("Zone sync is not configured; set ZoneSyncApi:Token."));
        }

        if (!Request.Headers.TryGetValue("Authorization", out var header))
        {
            return Task.FromResult(AuthenticateResult.NoResult());
        }

        var value = header.ToString();
        if (!value.StartsWith("Bearer ", StringComparison.Ordinal))
        {
            return Task.FromResult(AuthenticateResult.NoResult());
        }

        var provided = value["Bearer ".Length..];

        if (!FixedTimeEquals(provided, expected))
        {
            return Task.FromResult(AuthenticateResult.Fail("Invalid zone sync token."));
        }

        var identity = new ClaimsIdentity([new Claim(ClaimTypes.Name, "hermes-fleet")], AuthConstants.ZoneSyncScheme);

        return Task.FromResult(AuthenticateResult.Success(new AuthenticationTicket(new ClaimsPrincipal(identity), AuthConstants.ZoneSyncScheme)));
    }

    private static bool FixedTimeEquals(string a, string b) =>
        System.Security.Cryptography.CryptographicOperations.FixedTimeEquals(
            System.Text.Encoding.UTF8.GetBytes(a),
            System.Text.Encoding.UTF8.GetBytes(b));
}
