using Hermes.Mgmt.Common.Contracts.V1;
using Hermes.Mgmt.Common.Domain;
using Hermes.Mgmt.Common.Errors;
using Hermes.Mgmt.Service;
using Hermes.Mgmt.Service.Validation;

namespace Hermes.Mgmt.Api.Mapping;

public static class ContractMapper
{
    public static ProfileResponse ToResponse(this UserProfile profile) => new(
        profile.Id,
        profile.Email,
        profile.DisplayName,
        profile.Company,
        profile.Timezone,
        profile.NotificationEmail,
        profile.NotifyOnZoneErrors,
        profile.CreatedAt);

    public static UpdateProfileCommand ToCommand(this UpdateProfileRequest request) => new(
        request.DisplayName,
        request.Company,
        request.Timezone,
        request.NotificationEmail,
        request.NotifyOnZoneErrors);

    private static string ToWire(this ZoneStatus status) => status.ToString().ToLowerInvariant();

    public static ZoneResponse ToResponse(this Zone zone) => new(
        zone.Id,
        zone.Apex,
        zone.Status.ToWire(),
        zone.StatusMessage,
        zone.Version,
        zone.Nameservers,
        zone.RecordCount,
        zone.CreatedAt,
        zone.UpdatedAt);

    public static ZoneDetailResponse ToDetailResponse(this Zone zone) => new(
        zone.Id,
        zone.Apex,
        zone.Status.ToWire(),
        zone.StatusMessage,
        zone.Version,
        zone.Nameservers,
        zone.RecordCount,
        zone.CreatedAt,
        zone.UpdatedAt,
        zone.Soa.ToModel());

    public static SoaModel ToModel(this Soa soa) => new()
    {
        Mname = soa.Mname,
        Rname = soa.Rname,
        Serial = soa.Serial.Value,
        Refresh = soa.Refresh,
        Retry = soa.Retry,
        Expire = soa.Expire,
        Minimum = soa.Minimum,
        Ttl = soa.Ttl
    };

    public static Soa ToDomain(this SoaModel model) => new()
    {
        Mname = model.Mname,
        Rname = model.Rname,
        Serial = new Serial(model.Serial),
        Refresh = model.Refresh,
        Retry = model.Retry,
        Expire = model.Expire,
        Minimum = model.Minimum,
        Ttl = model.Ttl
    };

    public static DelegationCheckResponse ToResponse(this DelegationCheck check) => new(
        check.ObservedNameservers,
        check.ExpectedNameservers,
        check.Delegated,
        check.CheckedAt);

    public static RecordResponse ToResponse(this DnsRecord record) => new(
        record.Id,
        record.ZoneId,
        record.Name,
        record.Type.ToString(),
        record.Ttl,
        record.Data,
        record.Preference,
        record.Priority,
        record.Weight,
        record.Port,
        record.Flags,
        record.Tag,
        record.Notes,
        record.CreatedAt,
        record.UpdatedAt);

    public static RecordDraft ToDraft(this RecordRequest request)
    {
        if (!DnsValidator.TryParseType(request.Type, out var type))
        {
            throw AppException.Validation("type", $"'{request.Type}' is not a supported record type.");
        }

        if (!DnsValidator.IsClientSubmittable(type))
        {
            throw AppException.Validation("type", "ALIAS cannot be set directly. Create a CNAME at the apex and it will be stored as one.");
        }

        return new RecordDraft
        {
            Name = request.Name,
            Type = type,
            Ttl = request.Ttl,
            Data = request.Data,
            Preference = request.Preference,
            Priority = request.Priority,
            Weight = request.Weight,
            Port = request.Port,
            Flags = request.Flags,
            Tag = request.Tag,
            Notes = request.Notes
        };
    }
}
