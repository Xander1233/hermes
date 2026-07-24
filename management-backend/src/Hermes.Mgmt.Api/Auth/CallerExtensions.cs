using System.Security.Claims;
using Hermes.Mgmt.Service;

namespace Hermes.Mgmt.Api.Auth;

public static class CallerExtensions
{
    public static Caller ToCaller(this ClaimsPrincipal principal)
    {
        var userId = principal.FindFirstValue("sub")
                     ?? principal.FindFirstValue(ClaimTypes.NameIdentifier)
                     ?? throw new InvalidOperationException("Authenticated token has no 'sub' claim; cannot identify the caller.");

        var email = principal.FindFirstValue("email")
                    ?? principal.FindFirstValue(ClaimTypes.Email)
                    ?? string.Empty;

        var displayName = principal.FindFirstValue("name")
                          ?? principal.FindFirstValue("cognito:username");

        return new Caller(userId, email, displayName);
    }
}
