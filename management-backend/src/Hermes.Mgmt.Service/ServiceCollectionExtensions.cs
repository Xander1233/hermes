using Amazon.CognitoIdentityProvider;
using Hermes.Mgmt.Service.Delegation;
using Hermes.Mgmt.Service.Identity;
using Hermes.Mgmt.Service.Infrastructure;
using Hermes.Mgmt.Service.Options;
using Microsoft.Extensions.Configuration;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;

namespace Hermes.Mgmt.Service;

public static class ServiceCollectionExtensions
{
    public static IServiceCollection AddHermesServices(this IServiceCollection services, IConfiguration configuration)
    {
        services.AddOptions<ZoneDefaultsOptions>()
            .Bind(configuration.GetSection(ZoneDefaultsOptions.Section))
            .ValidateDataAnnotations()
            .ValidateOnStart();

        services.AddOptions<CognitoOptions>()
            .Bind(configuration.GetSection(CognitoOptions.Section))
            .ValidateDataAnnotations()
            .ValidateOnStart();

        services.Configure<HermesFleetOptions>(configuration.GetSection(HermesFleetOptions.Section));
        services.Configure<CacheOptions>(configuration.GetSection(CacheOptions.Section));
        services.Configure<DelegationOptions>(configuration.GetSection(DelegationOptions.Section));

        services.TryAddSingleton(TimeProvider.System);

        services.TryAddSingleton<IAmazonCognitoIdentityProvider>(_ => new AmazonCognitoIdentityProviderClient());
        services.TryAddSingleton<ICognitoIdentityService, CognitoIdentityService>();

        services.AddScoped<IProfileService, ProfileService>();
        services.AddScoped<IZoneService, ZoneService>();
        services.AddScoped<IRecordService, RecordService>();
        services.AddScoped<IZoneSyncService, ZoneSyncService>();
        services.AddScoped<ZonePublisher>();

        AddDelegationChecker(services, configuration);
        AddNotifier(services, configuration);

        services.AddScoped<DelegationScanner>();
        services.AddHostedService<DelegationMonitorService>();

        return services;
    }

    private static void AddDelegationChecker(IServiceCollection services, IConfiguration configuration)
    {
        var options = configuration.GetSection(DelegationOptions.Section).Get<DelegationOptions>() ?? new DelegationOptions();

        if (options.Enabled)
        {
            services.TryAddSingleton<IDelegationChecker, DelegationChecker>();
        }
        else
        {
            services.TryAddSingleton<IDelegationChecker, DisabledDelegationChecker>();
        }
    }

    private static void AddNotifier(IServiceCollection services, IConfiguration configuration)
    {
        var options = configuration.GetSection(HermesFleetOptions.Section).Get<HermesFleetOptions>() ?? new HermesFleetOptions();

        if (options.AdminEndpoints.Length == 0)
        {
            services.TryAddSingleton<IHermesNotifier, NoOpHermesNotifier>();
            return;
        }

        services.AddHttpClient(HermesNotifier.HttpClientName, client =>
        {
            client.Timeout = TimeSpan.FromMilliseconds(Math.Max(200, options.TimeoutMs));
        });

        services.TryAddSingleton<IHermesNotifier, HermesNotifier>();
    }
}
