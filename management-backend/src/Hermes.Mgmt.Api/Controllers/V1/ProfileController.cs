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
[Route("api/v{version:apiVersion}/me")]
[Authorize(Policy = AuthConstants.DashboardPolicy)]
[Produces("application/json")]
public sealed class ProfileController(IProfileService profiles) : ControllerBase
{
    [HttpGet]
    [ProducesResponseType<ProfileResponse>(StatusCodes.Status200OK)]
    public async Task<ActionResult<ProfileResponse>> Get(CancellationToken ct)
    {
        var profile = await profiles.GetOrCreateAsync(User.ToCaller(), ct);
        
        return Ok(profile.ToResponse());
    }

    [HttpPatch]
    [ProducesResponseType<ProfileResponse>(StatusCodes.Status200OK)]
    [ProducesResponseType<ProblemResponse>(StatusCodes.Status422UnprocessableEntity)]
    public async Task<ActionResult<ProfileResponse>> Update([FromBody] UpdateProfileRequest request, CancellationToken ct)
    {
        var profile = await profiles.UpdateAsync(User.ToCaller(), request.ToCommand(), ct);
        
        return Ok(profile.ToResponse());
    }
}
