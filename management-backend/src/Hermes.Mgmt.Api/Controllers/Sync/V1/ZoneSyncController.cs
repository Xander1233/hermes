using Asp.Versioning;
using Hermes.Mgmt.Api.Auth;
using Hermes.Mgmt.Common.Contracts.Sync.V1;
using Hermes.Mgmt.Service;
using Microsoft.AspNetCore.Authorization;
using Microsoft.AspNetCore.Mvc;

namespace Hermes.Mgmt.Api.Controllers.Sync.V1;

[ApiController]
[ApiVersion("1.0")]
[Route("sync/v{version:apiVersion}/zones")]
[Authorize(Policy = AuthConstants.ZoneSyncPolicy)]
[Produces("application/json")]
public sealed class ZoneSyncController(IZoneSyncService sync) : ControllerBase
{
    [HttpGet]
    [ProducesResponseType<ZoneManifestResponse>(StatusCodes.Status200OK)]
    public async Task<ActionResult<ZoneManifestResponse>> GetManifest(CancellationToken ct) => Ok(await sync.GetManifestAsync(ct));

    [HttpGet("{zoneId}")]
    [ProducesResponseType<ZoneDocumentResponse>(StatusCodes.Status200OK)]
    [ProducesResponseType(StatusCodes.Status404NotFound)]
    public async Task<ActionResult<ZoneDocumentResponse>> GetZone(string zoneId, CancellationToken ct)
    {
        var document = await sync.GetZoneDocumentAsync(zoneId, ct);
        
        return document is null ? NotFound() : Ok(document);
    }
}
