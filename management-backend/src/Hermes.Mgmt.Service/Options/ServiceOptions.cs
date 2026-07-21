using System.ComponentModel.DataAnnotations;

namespace Hermes.Mgmt.Service.Options;

public sealed class ZoneDefaultsOptions
{
    public const string Section = "ZoneDefaults";

    [MinLength(1)]
    public string[] Nameservers { get; set; } = [];
    public string? SoaMname { get; set; }
    public string SoaRnamePrefix { get; set; } = "hostmaster";

    public int Refresh { get; set; } = 3600;
    public int Retry { get; set; } = 600;
    public int Expire { get; set; } = 604800;
    public uint Minimum { get; set; } = 60;
    public uint SoaTtl { get; set; } = 3600;

    public int MaxZonesPerUser { get; set; } = 50;
    public int MaxRecordsPerZone { get; set; } = 1000;
}

public sealed class HermesFleetOptions
{
    public const string Section = "HermesFleet";
    
    public string[] AdminEndpoints { get; set; } = [];
    public string? AdminToken { get; set; }

    public int TimeoutMs { get; set; } = 3000;
}

public sealed class CacheOptions
{
    public const string Section = "Cache";

    /// <summary><c>None</c> (default) or <c>Redis</c>.</summary>
    public string Provider { get; set; } = "None";

    /// <summary>StackExchange.Redis connection string. Required when Provider is Redis.</summary>
    public string? RedisConnectionString { get; set; }

    public int TtlSeconds { get; set; } = 10;

    public string InstanceName { get; set; } = "hermes-mgmt:";
}

public sealed class DelegationOptions
{
    public const string Section = "Delegation";

    public bool Enabled { get; set; } = true;

    public string[] Resolvers { get; set; } = ["1.1.1.1", "8.8.8.8"];

    public int TimeoutMs { get; set; } = 3000;
    public bool MonitorEnabled { get; set; } = true;

    public int ScanIntervalSeconds { get; set; } = 60;

    public int BatchSize { get; set; } = 50;
}

public sealed class CognitoOptions
{
    public const string Section = "Cognito";

    [Required]
    public string UserPoolId { get; set; } = string.Empty;

    [Required]
    public string ClientId { get; set; } = string.Empty;

    [Required]
    public string Region { get; set; } = string.Empty;

    public string Authority => $"https://cognito-idp.{Region}.amazonaws.com/{UserPoolId}";

    public int ProviderCacheSeconds { get; set; } = 300;
}
