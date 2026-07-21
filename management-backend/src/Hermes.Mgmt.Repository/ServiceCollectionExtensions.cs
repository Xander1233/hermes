using Amazon;
using Amazon.DynamoDBv2;
using Hermes.Mgmt.Common.Abstractions;
using Hermes.Mgmt.Repository.Caching;
using Hermes.Mgmt.Repository.DynamoDb;
using Microsoft.Extensions.Configuration;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.DependencyInjection.Extensions;

namespace Hermes.Mgmt.Repository;

public static class ServiceCollectionExtensions
{
    public static IServiceCollection AddHermesRepositories(this IServiceCollection services, IConfiguration configuration)
    {
        services.Configure<DynamoDbOptions>(configuration.GetSection(DynamoDbOptions.Section));

        var dynamo = configuration.GetSection(DynamoDbOptions.Section).Get<DynamoDbOptions>() ?? new DynamoDbOptions();

        services.TryAddSingleton<IAmazonDynamoDB>(_ => CreateClient(dynamo));

        services.TryAddScoped<IProfileRepository, DynamoDbProfileRepository>();
        services.TryAddScoped<IZoneRepository, DynamoDbZoneRepository>();
        services.TryAddScoped<IRecordRepository, DynamoDbRecordRepository>();

        AddCache(services, configuration);

        return services;
    }

    private static IAmazonDynamoDB CreateClient(DynamoDbOptions options)
    {
        var config = new AmazonDynamoDBConfig();

        if (!string.IsNullOrWhiteSpace(options.ServiceUrl))
        {
            config.ServiceURL = options.ServiceUrl;
            config.AuthenticationRegion = options.Region ?? "us-east-1";
        }
        else if (!string.IsNullOrWhiteSpace(options.Region))
        {
            config.RegionEndpoint = RegionEndpoint.GetBySystemName(options.Region);
        }

        return new AmazonDynamoDBClient(config);
    }

    private static void AddCache(IServiceCollection services, IConfiguration configuration)
    {
        var provider = configuration["Cache:Provider"];

        if (!string.Equals(provider, "Redis", StringComparison.OrdinalIgnoreCase))
        {
            services.TryAddSingleton<ICacheProvider, NoOpCacheProvider>();
            return;
        }

        var connectionString = configuration["Cache:RedisConnectionString"];
        if (string.IsNullOrWhiteSpace(connectionString))
        {
            throw new InvalidOperationException("Cache:Provider is 'Redis' but Cache:RedisConnectionString is not set.");
        }

        services.AddStackExchangeRedisCache(cache =>
        {
            cache.Configuration = connectionString;
            cache.InstanceName = configuration["Cache:InstanceName"] ?? "hermes-mgmt:";
        });

        services.TryAddSingleton<ICacheProvider, RedisCacheProvider>();
    }
}
