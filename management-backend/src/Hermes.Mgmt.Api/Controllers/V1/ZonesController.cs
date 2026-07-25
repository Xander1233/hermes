using Asp.Versioning;
using Hermes.Mgmt.Api.Auth;
using Hermes.Mgmt.Api.Mapping;
using Hermes.Mgmt.Common.Contracts.V1;
using Hermes.Mgmt.Service;
using Microsoft.AspNetCore.Authorization;
using Microsoft.AspNetCore.Mvc;

namespace Hermes.Mgmt.Api.Controllers.V1;

[ApiController]
[ApiVersion("1.0")]
[Route("api/v{version:apiVersion}/zones")]
[Authorize(Policy = AuthConstants.DashboardPolicy)]
[Produces("application/json")]
public sealed class ZonesController(IZoneService zones) : ControllerBase
{
    [HttpGet]
    [ProducesResponseType<IReadOnlyList<ZoneResponse>>(StatusCodes.Status200OK)]
    public async Task<ActionResult<IReadOnlyList<ZoneResponse>>> List(CancellationToken ct)
    {
        var owned = await zones.ListAsync(UserId, ct);
        
        return Ok(owned.Select(z => z.ToResponse()).ToList());
    }

    [HttpPost]
    [ProducesResponseType<ZoneDetailResponse>(StatusCodes.Status201Created)]
    [ProducesResponseType<ProblemResponse>(StatusCodes.Status409Conflict)]
    [ProducesResponseType<ProblemResponse>(StatusCodes.Status422UnprocessableEntity)]
    public async Task<ActionResult<ZoneDetailResponse>> Create([FromBody] CreateZoneRequest request, ApiVersion version, CancellationToken ct)
    {
        var zone = await zones.CreateAsync(UserId, request.Apex, ct);

        return CreatedAtAction(nameof(Get), new { version = version.ToString(), zoneId = zone.Id }, zone.ToDetailResponse());
    }

    [HttpGet("{zoneId}")]
    [ProducesResponseType<ZoneDetailResponse>(StatusCodes.Status200OK)]
    [ProducesResponseType<ProblemResponse>(StatusCodes.Status404NotFound)]
    public async Task<ActionResult<ZoneDetailResponse>> Get(string zoneId, CancellationToken ct)
    {
        var zone = await zones.GetAsync(UserId, zoneId, ct);
        
        return Ok(zone.ToDetailResponse());
    }

    [HttpDelete("{zoneId}")]
    [ProducesResponseType(StatusCodes.Status204NoContent)]
    [ProducesResponseType<ProblemResponse>(StatusCodes.Status404NotFound)]
    public async Task<IActionResult> Delete(string zoneId, CancellationToken ct)
    {
        await zones.DeleteAsync(UserId, zoneId, ct);
        
        return NoContent();
    }

    [HttpPut("{zoneId}/soa")]
    [ProducesResponseType<ZoneDetailResponse>(StatusCodes.Status200OK)]
    [ProducesResponseType<ProblemResponse>(StatusCodes.Status404NotFound)]
    [ProducesResponseType<ProblemResponse>(StatusCodes.Status422UnprocessableEntity)]
    public async Task<ActionResult<ZoneDetailResponse>> UpdateSoa(string zoneId, [FromBody] SoaModel soa, CancellationToken ct)
    {
        var zone = await zones.UpdateSoaAsync(UserId, zoneId, soa.ToDomain(), ct);
        
        return Ok(zone.ToDetailResponse());
    }

    [HttpGet("{zoneId}/delegation")]
    [ProducesResponseType<DelegationCheckResponse>(StatusCodes.Status200OK)]
    [ProducesResponseType<ProblemResponse>(StatusCodes.Status404NotFound)]
    public async Task<ActionResult<DelegationCheckResponse>> CheckDelegation(string zoneId, CancellationToken ct)
    {
        var check = await zones.CheckDelegationAsync(UserId, zoneId, ct);
        
        return Ok(check.ToResponse());
    }

    private string UserId => User.ToCaller().UserId;
}
