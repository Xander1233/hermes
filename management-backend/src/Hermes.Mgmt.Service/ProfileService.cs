using Hermes.Mgmt.Common.Abstractions;
using Hermes.Mgmt.Common.Domain;
using Hermes.Mgmt.Common.Errors;

namespace Hermes.Mgmt.Service;

public sealed class ProfileService(IProfileRepository profiles, TimeProvider clock) : IProfileService
{
    public async Task<UserProfile> GetOrCreateAsync(Caller caller, CancellationToken ct = default)
    {
        var existing = await profiles.GetAsync(caller.UserId, ct);
        if (existing is not null)
        {
            return existing.Email == caller.Email ? existing : await Persist(existing with { Email = caller.Email }, ct);
        }

        var profile = new UserProfile
        {
            Id = caller.UserId,
            Email = caller.Email,
            DisplayName = string.IsNullOrWhiteSpace(caller.DisplayName) ? DeriveDisplayName(caller.Email) : caller.DisplayName.Trim(),
            Company = null,
            Timezone = "UTC",
            NotificationEmail = null,
            NotifyOnZoneErrors = true,
            CreatedAt = clock.GetUtcNow()
        };

        return await Persist(profile, ct);
    }

    public async Task<UserProfile> UpdateAsync(Caller caller, UpdateProfileCommand command, CancellationToken ct = default)
    {
        var profile = await GetOrCreateAsync(caller, ct);

        var displayName = command.DisplayName.Trim();
        if (displayName.Length == 0)
        {
            throw AppException.Validation("displayName", "Display name is required.");
        }

        var timezone = command.Timezone.Trim();
        if (!IsKnownTimezone(timezone))
        {
            throw AppException.Validation("timezone", "Not a recognized IANA timezone.");
        }

        var updated = profile with
        {
            DisplayName = displayName,
            Company = Blank(command.Company),
            Timezone = timezone,
            NotificationEmail = Blank(command.NotificationEmail),
            NotifyOnZoneErrors = command.NotifyOnZoneErrors
        };

        return await Persist(updated, ct);
    }

    private async Task<UserProfile> Persist(UserProfile profile, CancellationToken ct)
    {
        await profiles.PutAsync(profile, ct);
        
        return profile;
    }

    private static string? Blank(string? value) => string.IsNullOrWhiteSpace(value) ? null : value.Trim();

    private static string DeriveDisplayName(string email)
    {
        var local = email.Split('@')[0];
        
        return local.Length == 0 ? "User" : local;
    }

    private static bool IsKnownTimezone(string id)
    {
        if (id.Length == 0)
        {
            return false;
        }

        return TimeZoneInfo.TryFindSystemTimeZoneById(id, out _);
    }
}
