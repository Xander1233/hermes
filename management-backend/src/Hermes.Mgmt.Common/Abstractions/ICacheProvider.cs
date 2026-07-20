namespace Hermes.Mgmt.Common.Abstractions;

public interface ICacheProvider
{
    Task<T?> GetAsync<T>(string key, CancellationToken ct = default) where T : class;

    Task SetAsync<T>(string key, T value, TimeSpan ttl, CancellationToken ct = default) where T : class;

    Task RemoveAsync(string key, CancellationToken ct = default);
}

public static class CacheKeys
{
    public const string Manifest = "sync:v1:manifest";

    public static string ZoneDocument(string zoneId) => $"sync:v1:zone:{zoneId}";
}
