using Hermes.Mgmt.Service.Options;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;
using Microsoft.Extensions.Options;

namespace Hermes.Mgmt.Service.Delegation;

public sealed class DelegationMonitorService(IServiceScopeFactory scopeFactory, IOptions<DelegationOptions> options, TimeProvider clock, ILogger<DelegationMonitorService> logger) : BackgroundService
{
    private readonly DelegationOptions _options = options.Value;

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        if (!_options.Enabled || !_options.MonitorEnabled)
        {
            logger.LogInformation("Delegation monitor is disabled; zones activate only when someone checks from the dashboard");
            return;
        }

        var interval = TimeSpan.FromSeconds(Math.Max(10, _options.ScanIntervalSeconds));
        logger.LogInformation("Delegation monitor started; scanning every {Interval}", interval);

        try
        {
            await Task.Delay(TimeSpan.FromMilliseconds(Random.Shared.Next(1000, 5000)), clock, stoppingToken);
        }
        catch (OperationCanceledException)
        {
            return;
        }

        using var timer = new PeriodicTimer(interval, clock);

        do
        {
            try
            {
                using var scope = scopeFactory.CreateScope();
                var scanner = scope.ServiceProvider.GetRequiredService<DelegationScanner>();
                await scanner.RunOnceAsync(stoppingToken);
            }
            catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested)
            {
                return;
            }
            catch (Exception ex)
            {
                logger.LogError(ex, "Delegation scan failed; retrying on the next interval");
            }
        }
        while (await SafeWaitAsync(timer, stoppingToken));

        logger.LogInformation("Delegation monitor stopped");
    }

    private static async Task<bool> SafeWaitAsync(PeriodicTimer timer, CancellationToken ct)
    {
        try
        {
            return await timer.WaitForNextTickAsync(ct);
        }
        catch (OperationCanceledException)
        {
            return false;
        }
    }
}
