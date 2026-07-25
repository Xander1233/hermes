using Asp.Versioning;
using Hermes.Mgmt.Service;
using Microsoft.AspNetCore.Authorization;
using Microsoft.AspNetCore.Mvc;

namespace Hermes.Mgmt.Api.Controllers.V1;

[ApiController]
[ApiVersion("1.0")]
[Route("api/v{version:apiVersion}/auth")]
[AllowAnonymous]
[Produces("application/json")]
public sealed class AuthController(ICognitoIdentityService identities) : ControllerBase
{
    [HttpGet("providers")]
    [ProducesResponseType<IReadOnlyList<string>>(StatusCodes.Status200OK)]
    public async Task<ActionResult<IReadOnlyList<string>>> Providers(CancellationToken ct)
    {
        return Ok(await identities.ListEnabledProvidersAsync(ct));
    }
}
