using System.Text.Json.Serialization;
using Amazon.DynamoDBv2;
using Asp.Versioning;
using Hermes.Mgmt.Api.Auth;
using Hermes.Mgmt.Api.Middleware;
using Hermes.Mgmt.Repository;
using Hermes.Mgmt.Repository.DynamoDb;
using Hermes.Mgmt.Service;
using Microsoft.Extensions.Options;
using Microsoft.IdentityModel.Tokens;

var builder = WebApplication.CreateBuilder(args);

builder.Host.UseDefaultServiceProvider(options =>
{
    options.ValidateScopes = true;
    options.ValidateOnBuild = true;
});

builder.Services.AddHermesRepositories(builder.Configuration);
builder.Services.AddHermesServices(builder.Configuration);

builder.Services
    .AddControllers()
    .AddJsonOptions(json =>
    {
        json.JsonSerializerOptions.PropertyNamingPolicy = System.Text.Json.JsonNamingPolicy.CamelCase;
        json.JsonSerializerOptions.DefaultIgnoreCondition = JsonIgnoreCondition.Never;
    });

builder.Services
    .AddApiVersioning(options =>
    {
        options.DefaultApiVersion = new ApiVersion(1, 0);
        options.AssumeDefaultVersionWhenUnspecified = true;
        options.ReportApiVersions = true;
        options.ApiVersionReader = new UrlSegmentApiVersionReader();
    })
    .AddMvc()
    .AddApiExplorer(options =>
    {
        options.GroupNameFormat = "'v'VVV";
        options.SubstituteApiVersionInUrl = true;
    });

var cognito = builder.Configuration.GetSection("Cognito");
var userPoolId = cognito["UserPoolId"];
var region = cognito["Region"];
var clientId = cognito["ClientId"];

builder.Services
    .AddAuthentication(AuthConstants.DashboardScheme)
    .AddJwtBearer(AuthConstants.DashboardScheme, options =>
    {
        options.Authority = $"https://cognito-idp.{region}.amazonaws.com/{userPoolId}";

        options.TokenValidationParameters = new TokenValidationParameters
        {
            ValidateIssuer = true,
            ValidIssuer = options.Authority,
            ValidateIssuerSigningKey = true,
            ValidateLifetime = true,
            ClockSkew = TimeSpan.FromMinutes(2),
            ValidateAudience = !string.IsNullOrWhiteSpace(clientId),
            ValidAudience = clientId,
            NameClaimType = "sub"
        };

        options.MapInboundClaims = false;
    })
    .AddScheme<ZoneSyncAuthOptions, ZoneSyncAuthenticationHandler>(AuthConstants.ZoneSyncScheme, options => 
        builder.Configuration.GetSection(ZoneSyncAuthOptions.Section).Bind(options));

builder.Services.AddAuthorizationBuilder()
    .AddPolicy(AuthConstants.DashboardPolicy, policy =>
    {
        policy.AddAuthenticationSchemes(AuthConstants.DashboardScheme);
        policy.RequireAuthenticatedUser();
    })
    .AddPolicy(AuthConstants.ZoneSyncPolicy, policy =>
    {
        policy.AddAuthenticationSchemes(AuthConstants.ZoneSyncScheme);
        policy.RequireAuthenticatedUser();
    });

var allowedOrigins = builder.Configuration.GetSection("Cors:AllowedOrigins").Get<string[]>() ?? [];
builder.Services.AddCors(options => options.AddDefaultPolicy(policy =>
{
    if (allowedOrigins.Length > 0)
    {
        policy.WithOrigins(allowedOrigins).AllowAnyHeader().AllowAnyMethod();
    }
}));

builder.Services.AddEndpointsApiExplorer();
builder.Services.AddSwaggerGen();
builder.Services.AddHealthChecks();

var app = builder.Build();

app.UseMiddleware<ExceptionHandlingMiddleware>();

if (app.Environment.IsDevelopment())
{
    app.UseSwagger();
    app.UseSwaggerUI();
    await EnsureTableAsync(app);
}

app.UseCors();
app.UseAuthentication();
app.UseAuthorization();

app.MapControllers();
app.MapHealthChecks("/healthz").AllowAnonymous();

app.Run();

// dev-only
static async Task EnsureTableAsync(WebApplication app)
{
    var options = app.Services.GetRequiredService<IOptions<DynamoDbOptions>>().Value;
    if (!options.CreateTableIfMissing)
    {
        return;
    }

    var client = app.Services.GetRequiredService<IAmazonDynamoDB>();
    var logger = app.Services.GetRequiredService<ILoggerFactory>().CreateLogger("Startup");

    try
    {
        var tables = await client.ListTablesAsync();
        if (tables.TableNames.Contains(options.TableName))
        {
            return;
        }

        await client.CreateTableAsync(TableSchema.CreateTableRequest(options.TableName));
        logger.LogInformation("Created DynamoDB table {Table}", options.TableName);
    }
    catch (Exception ex)
    {
        logger.LogWarning(ex, "Could not ensure DynamoDB table {Table} exists", options.TableName);
    }
}

/// <summary>Exposed so the integration tests can host the real pipeline.</summary>
public partial class Program;
