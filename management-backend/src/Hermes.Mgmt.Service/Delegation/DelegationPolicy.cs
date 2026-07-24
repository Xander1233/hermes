namespace Hermes.Mgmt.Service.Delegation;

public static class DelegationPolicy
{
    public static bool IsDelegated(IReadOnlyList<string> expected, IReadOnlyList<string> observed)
    {
        if (expected.Count == 0 || observed.Count == 0)
        {
            return false;
        }

        return expected.All(ns => observed.Contains(ns, StringComparer.OrdinalIgnoreCase));
    }

    public static DateTimeOffset NextCheckAt(DateTimeOffset createdAt, DateTimeOffset now)
    {
        var age = now - createdAt;

        var interval = age switch
        {
            _ when age < TimeSpan.FromHours(1) => TimeSpan.FromMinutes(2),
            _ when age < TimeSpan.FromDays(1) => TimeSpan.FromMinutes(15),
            _ when age < TimeSpan.FromDays(7) => TimeSpan.FromHours(1),
            _ => TimeSpan.FromHours(6)
        };

        return now + interval;
    }
}
