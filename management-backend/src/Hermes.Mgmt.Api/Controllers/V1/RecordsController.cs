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
[Route("api/v{version:apiVersion}/zones/{zoneId}/records")]
[Authorize(Policy = AuthConstants.DashboardPolicy)]
[Produces("application/json")]
public sealed class RecordsController(IRecordService records) : ControllerBase
{
    [HttpGet]
    [ProducesResponseType<IReadOnlyList<RecordResponse>>(StatusCodes.Status200OK)]
    [ProducesResponseType<ProblemResponse>(StatusCodes.Status404NotFound)]
    public async Task<ActionResult<IReadOnlyList<RecordResponse>>> List(string zoneId, CancellationToken ct)
    {
        var found = await records.ListAsync(UserId, zoneId, ct);
        
        return Ok(found.Select(r => r.ToResponse()).ToList());
    }

    [HttpPost]
    [ProducesResponseType<RecordResponse>(StatusCodes.Status201Created)]
    [ProducesResponseType<ProblemResponse>(StatusCodes.Status404NotFound)]
    [ProducesResponseType<ProblemResponse>(StatusCodes.Status409Conflict)]
    [ProducesResponseType<ProblemResponse>(StatusCodes.Status422UnprocessableEntity)]
    public async Task<ActionResult<RecordResponse>> Create(string zoneId, [FromBody] RecordRequest request, ApiVersion version, CancellationToken ct)
    {
        var record = await records.CreateAsync(UserId, zoneId, request.ToDraft(), ct);

        return CreatedAtAction(nameof(Get), new { version = version.ToString(), zoneId, recordId = record.Id }, record.ToResponse());
    }

    [HttpGet("{recordId}")]
    [ProducesResponseType<RecordResponse>(StatusCodes.Status200OK)]
    [ProducesResponseType<ProblemResponse>(StatusCodes.Status404NotFound)]
    public async Task<ActionResult<RecordResponse>> Get(string zoneId, string recordId, CancellationToken ct)
    {
        var all = await records.ListAsync(UserId, zoneId, ct);
        var record = all.FirstOrDefault(r => r.Id == recordId);

        return record is null
            ? throw Common.Errors.AppException.NotFound("Record")
            : Ok(record.ToResponse());
    }

    [HttpPut("{recordId}")]
    [ProducesResponseType<RecordResponse>(StatusCodes.Status200OK)]
    [ProducesResponseType<ProblemResponse>(StatusCodes.Status404NotFound)]
    [ProducesResponseType<ProblemResponse>(StatusCodes.Status409Conflict)]
    [ProducesResponseType<ProblemResponse>(StatusCodes.Status422UnprocessableEntity)]
    public async Task<ActionResult<RecordResponse>> Update(string zoneId, string recordId, [FromBody] RecordRequest request, CancellationToken ct)
    {
        var record = await records.UpdateAsync(UserId, zoneId, recordId, request.ToDraft(), ct);
        
        return Ok(record.ToResponse());
    }

    [HttpDelete("{recordId}")]
    [ProducesResponseType(StatusCodes.Status204NoContent)]
    [ProducesResponseType<ProblemResponse>(StatusCodes.Status404NotFound)]
    public async Task<IActionResult> Delete(string zoneId, string recordId, CancellationToken ct)
    {
        await records.DeleteAsync(UserId, zoneId, recordId, ct);
        
        return NoContent();
    }

    private string UserId => User.ToCaller().UserId;
}
