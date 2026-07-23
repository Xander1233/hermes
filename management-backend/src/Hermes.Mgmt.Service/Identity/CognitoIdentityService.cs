using Amazon.CognitoIdentityProvider;
using Amazon.CognitoIdentityProvider.Model;
using Hermes.Mgmt.Service.Options;
using Microsoft.Extensions.Options;

namespace Hermes.Mgmt.Service.Identity;

public sealed class CognitoIdentityService(IAmazonCognitoIdentityProvider cognito, IOptions<CognitoOptions> options, TimeProvider clock) : ICognitoIdentityService
{
    private readonly CognitoOptions _options = options.Value;

    private readonly SemaphoreSlim _providerLock = new(1, 1);
    private IReadOnlyList<string>? _providers;
    private DateTimeOffset _providersExpireAt;

    public async Task<IReadOnlyList<string>> ListEnabledProvidersAsync(CancellationToken ct = default)
    {
        if (TryGetCachedProviders(out var cached))
        {
            return cached;
        }

        await _providerLock.WaitAsync(ct);
        try
        {
            if (TryGetCachedProviders(out cached))
            {
                return cached;
            }

            var response = await cognito.DescribeUserPoolClientAsync(
                new DescribeUserPoolClientRequest
                {
                    UserPoolId = _options.UserPoolId,
                    ClientId = _options.ClientId,
                },
                ct);

            _providers = response.UserPoolClient?.SupportedIdentityProviders ?? [];
            _providersExpireAt = clock.GetUtcNow().AddSeconds(_options.ProviderCacheSeconds);
            return _providers;
        }
        catch (Exception ex)
        {
            Console.WriteLine(ex.Message);
            return Array.Empty<string>();
        }
        finally
        {
            _providerLock.Release();
        }
    }

    private bool TryGetCachedProviders(out IReadOnlyList<string> providers)
    {
        providers = _providers ?? [];
        return _providers is not null && clock.GetUtcNow() < _providersExpireAt;
    }
}
