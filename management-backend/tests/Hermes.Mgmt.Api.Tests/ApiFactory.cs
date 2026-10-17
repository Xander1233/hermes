using System.Net.Http.Headers;
using System.Security.Claims;
using System.Text.Encodings.Web;
using Hermes.Mgmt.Api.Auth;
using Hermes.Mgmt.Common.Abstractions;
using Hermes.Mgmt.Service;
using Hermes.Mgmt.TestSupport;
using Microsoft.AspNetCore.Authentication;
using Microsoft.AspNetCore.Hosting;
using Microsoft.AspNetCore.Mvc.Testing;
using Microsoft.AspNetCore.TestHost;
using Microsoft.Extensions.Configuration;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;
using Microsoft.Extensions.Logging;
using Microsoft.Extensions.Options;

namespace Hermes.Mgmt.Api.Tests;

/// <summary>
/// Hosts the real API pipeline over in-memory persistence.
/// </summary>
/// <remarks>
/// <para>
/// Everything that makes these integration tests worth having stays real: routing, API
/// versioning, model binding, JSON casing, the authorization policies, and the exception
/// middleware that shapes error bodies. Only two things are swapped —
/// </para>
/// <list type="bullet">
///   <item>DynamoDB, for an in-memory store, so tests need no AWS and no container;</item>
///   <item>Cognito's JWT validation, for a test scheme, so tests need no user pool and no
///   network call to fetch a JWKS. The <em>policies</em> are untouched, so "this endpoint
///   requires authentication" is still genuinely under test.</item>
/// </list>
/// <para>
/// The zone-sync scheme is <em>not</em> stubbed: it is a static token compared in-process, so
/// the tests exercise the real handler, including its reject paths.
/// </para>
/// </remarks>
public sealed class ApiFactory : WebApplicationFactory<Program>
{
    public const string ZoneSyncToken = "test-zone-sync-token";
    public const string DefaultUserId = "user-1";

    public InMemoryStore Store { get; } = new();
    public RecordingNotifier Notifier { get; } = new();
    public StubDelegationChecker Delegation { get; } = new();

    protected override void ConfigureWebHost(IWebHostBuilder builder)
    {
        builder.UseEnvironment("Testing");

        builder.ConfigureAppConfiguration((_, config) =>
            config.AddInMemoryCollection(new Dictionary<string, string?>
            {
                ["ZoneSyncApi:Token"] = ZoneSyncToken,
                ["Delegation:Enabled"] = "false",
                ["Cache:Provider"] = "None",
                // ZoneDefaults has no usable defaults in code: Nameservers is [MinLength(1)]
                // and validated on start, so the host refuses to boot without it. It has to be
                // set here rather than left to appsettings.json, which is gitignored and so
                // absent on CI.
                ["ZoneDefaults:Nameservers:0"] = "ns1.test.invalid",
                ["ZoneDefaults:Nameservers:1"] = "ns2.test.invalid",
                // Present only so the JwtBearer handler can be constructed; the test auth
                // scheme below is what actually runs.
                ["Cognito:Region"] = "eu-central-1",
                ["Cognito:UserPoolId"] = "eu-central-1_test",
                ["Cognito:ClientId"] = "test-client"
            }));

        builder.ConfigureTestServices(services =>
        {
            services.RemoveAll<IProfileRepository>();
            services.RemoveAll<IZoneRepository>();
            services.RemoveAll<IRecordRepository>();
            services.RemoveAll<IHermesNotifier>();
            services.RemoveAll<IDelegationChecker>();
            services.RemoveAll<Amazon.DynamoDBv2.IAmazonDynamoDB>();

            services.AddSingleton(Store);
            services.AddScoped<IProfileRepository>(_ => new InMemoryProfileRepository(Store));
            services.AddScoped<IZoneRepository>(_ => new InMemoryZoneRepository(Store));
            services.AddScoped<IRecordRepository>(_ => new InMemoryRecordRepository(Store));
            services.AddSingleton<IHermesNotifier>(Notifier);
            services.AddSingleton<IDelegationChecker>(Delegation);

            // Swap the handler *behind* the Cognito scheme rather than registering a new
            // scheme: the name stays the same, so the authorization policies — which name that
            // scheme explicitly — are the real ones, and "this endpoint requires auth" is
            // genuinely under test. Re-adding the scheme would throw "Scheme already exists".
            services.PostConfigure<AuthenticationOptions>(options =>
                options.SchemeMap[AuthConstants.DashboardScheme].HandlerType =
                    typeof(TestAuthHandler));
        });
    }

    /// <summary>A client authenticated as a dashboard user.</summary>
    public HttpClient CreateUserClient(string userId = DefaultUserId, string email = "user@example.com")
    {
        var client = CreateClient();
        client.DefaultRequestHeaders.Add(TestAuthHandler.UserIdHeader, userId);
        client.DefaultRequestHeaders.Add(TestAuthHandler.EmailHeader, email);
        return client;
    }

    /// <summary>A client authenticated as the Hermes fleet.</summary>
    public HttpClient CreateSyncClient(string? token = ZoneSyncToken)
    {
        var client = CreateClient();
        if (token is not null)
        {
            client.DefaultRequestHeaders.Authorization = new AuthenticationHeaderValue("Bearer", token);
        }

        return client;
    }
}

/// <summary>
/// Stands in for Cognito's JWT validation, taking the identity from request headers.
/// </summary>
/// <remarks>
/// It emits exactly the claims <see cref="CallerExtensions"/> reads — <c>sub</c>, <c>email</c>,
/// <c>name</c> — so the projection from token to caller is exercised for real. A request with
/// no identity header is unauthenticated, which is what lets the tests assert that the
/// endpoints reject anonymous callers.
/// </remarks>
public sealed class TestAuthHandler(
    IOptionsMonitor<AuthenticationSchemeOptions> options,
    ILoggerFactory logger,
    UrlEncoder encoder) : AuthenticationHandler<AuthenticationSchemeOptions>(options, logger, encoder)
{
    public const string UserIdHeader = "X-Test-User";
    public const string EmailHeader = "X-Test-Email";

    protected override Task<AuthenticateResult> HandleAuthenticateAsync()
    {
        if (!Request.Headers.TryGetValue(UserIdHeader, out var userId))
        {
            return Task.FromResult(AuthenticateResult.NoResult());
        }

        var email = Request.Headers.TryGetValue(EmailHeader, out var value)
            ? value.ToString()
            : "user@example.com";

        var identity = new ClaimsIdentity(
        [
            new Claim("sub", userId.ToString()),
            new Claim("email", email),
            new Claim("name", "Test User")
        ], AuthConstants.DashboardScheme);

        return Task.FromResult(AuthenticateResult.Success(new AuthenticationTicket(
            new ClaimsPrincipal(identity), AuthConstants.DashboardScheme)));
    }
}
