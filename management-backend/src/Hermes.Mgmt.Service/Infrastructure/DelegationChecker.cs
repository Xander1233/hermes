using System.Net;
using DnsClient;
using DnsClient.Protocol;
using Hermes.Mgmt.Service.Options;
using Microsoft.Extensions.Logging;
using Microsoft.Extensions.Options;

namespace Hermes.Mgmt.Service.Infrastructure;

public sealed class DelegationChecker : IDelegationChecker
{
    private readonly ILookupClient _client;
    private readonly DelegationOptions _options;
    private readonly ILogger<DelegationChecker> _logger;

    public DelegationChecker(IOptions<DelegationOptions> options, ILogger<DelegationChecker> logger)
    {
        _options = options.Value;
        _logger = logger;

        var servers = _options.Resolvers
            .Select(r => IPAddress.TryParse(r, out var ip) ? ip : null)
            .OfType<IPAddress>()
            .ToArray();

        var settings = new LookupClientOptions(servers)
        {
            Timeout = TimeSpan.FromMilliseconds(Math.Max(200, _options.TimeoutMs)),
            UseCache = false,
            Recursion = true,
            Retries = 1
        };

        _client = new LookupClient(settings);
    }

    internal DelegationChecker(ILookupClient client, DelegationOptions options, ILogger<DelegationChecker> logger)
    {
        _client = client;
        _options = options;
        _logger = logger;
    }

    public async Task<IReadOnlyList<string>> LookupNameserversAsync(string apex, CancellationToken ct = default)
    {
        if (!_options.Enabled)
        {
            return [];
        }

        var response = await _client.QueryAsync(apex, QueryType.NS, cancellationToken: ct);

        if (response.HasError)
        {
            _logger.LogDebug("NS lookup for {Apex} returned {Error}", apex, response.ErrorMessage);
            return [];
        }

        var names = response.Answers.OfType<NsRecord>()
            .Concat(response.Authorities.OfType<NsRecord>())
            .Select(ns => Normalize(ns.NSDName.Value))
            .Where(n => n.Length > 0)
            .Distinct(StringComparer.OrdinalIgnoreCase)
            .OrderBy(n => n, StringComparer.Ordinal)
            .ToList();

        return names;
    }

    private static string Normalize(string name) => name.Trim().TrimEnd('.').ToLowerInvariant();
}

public sealed class DisabledDelegationChecker : IDelegationChecker
{
    public Task<IReadOnlyList<string>> LookupNameserversAsync(string apex, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<string>>([]);
}
