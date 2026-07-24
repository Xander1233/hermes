using System.Net.Http.Headers;
using Hermes.Mgmt.Service.Options;
using Microsoft.Extensions.Logging;
using Microsoft.Extensions.Options;

namespace Hermes.Mgmt.Service.Infrastructure;

public sealed class HermesNotifier(IHttpClientFactory httpClientFactory, IOptions<HermesFleetOptions> options, ILogger<HermesNotifier> logger) : IHermesNotifier
{
    public const string HttpClientName = "hermes-fleet";

    private readonly HermesFleetOptions _options = options.Value;

    public async Task NotifyAsync(CancellationToken ct = default)
    {
        if (_options.AdminEndpoints.Length == 0)
        {
            return;
        }

        if (string.IsNullOrWhiteSpace(_options.AdminToken))
        {
            logger.LogWarning("HermesFleet:AdminEndpoints is set without an AdminToken; reload notifications will be refused");
            return;
        }

        var client = httpClientFactory.CreateClient(HttpClientName);
        
        var sends = _options.AdminEndpoints.Select(endpoint => NotifyOneAsync(client, endpoint, ct));
        await Task.WhenAll(sends);
    }

    private async Task NotifyOneAsync(HttpClient client, string endpoint, CancellationToken ct)
    {
        logger.LogWarning("Endpoint url {endpoint} is being set", endpoint);
        var url = $"{endpoint.TrimEnd('/')}/admin/reload";

        try
        {
            using var request = new HttpRequestMessage(HttpMethod.Post, url);
            request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", _options.AdminToken);

            using var response = await client.SendAsync(request, ct);

            if (!response.IsSuccessStatusCode)
            {
                logger.LogWarning("Hermes reload at {Url} returned {Status}; change propagates on the next poll", url, (int)response.StatusCode);
            }
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception ex)
        {
            logger.LogWarning(ex, "Could not notify Hermes at {Url}; change propagates on the next poll", url);
        }
    }
}

public sealed class NoOpHermesNotifier : IHermesNotifier
{
    public Task NotifyAsync(CancellationToken ct = default) => Task.CompletedTask;
}
