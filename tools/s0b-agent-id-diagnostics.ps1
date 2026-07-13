<#
.SYNOPSIS
  Inspect the S0b Agent ID and resource-app configuration without exposing
  bearer credentials, and optionally scaffold a local Agent Identities minter.

.DESCRIPTION
  The script is read-only. It backs up the complete resource application
  manifest under .state (ignored by Git) for a support case. It never prints
  or writes a client secret, an access token, or a bearer header.

  Microsoft documentation used:
  - /entra/agent-id/agent-token-claims
  - /entra/agent-id/autonomous-agent-authentication-authorization-flow
  - /entra/identity-platform/optional-claims
  - /entra/identity-platform/optional-claims-reference

  Important: xms_sub_fct, xms_act_fct, and xms_par_app_azp are Agent ID token
  claims, but they are not names in Microsoft's supported optional-claims
  reference. Do not add them to optionalClaims. They are emitted by the
  supported autonomous Agent Identity acquisition flow.
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $TenantId,
    [Parameter(Mandatory)]
    [string] $AgentIdentityObjectId,
    [Parameter(Mandatory)]
    [string] $GatewayAppId,
    [string] $BackupDirectory = '.state/s0b-agent-id',
    [switch] $ScaffoldAgentIdentityMinter,
    [switch] $RunScaffoldedMinter,
    [string] $BlueprintClientId,
    [string] $Principal
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Require-GraphPowerShell {
    if (-not (Get-Command Connect-MgGraph -ErrorAction SilentlyContinue) -or
        -not (Get-Command Invoke-MgGraphRequest -ErrorAction SilentlyContinue)) {
        throw @'
Microsoft Graph PowerShell is required but is not installed in this process.
Install it outside this script (for example: Install-Module Microsoft.Graph -Scope CurrentUser),
then authenticate with Application.Read.All. This script will not install software,
modify Azure resources, or persist credentials.
'@
    }
}

function Require-GraphContext {
    if (-not (Get-MgContext)) {
        throw 'Microsoft Graph is not authenticated. Run Connect-MgGraph -Scopes Application.Read.All first.'
    }
}

function Get-AgentIdentity {
    $uri = "https://graph.microsoft.com/beta/servicePrincipals/$AgentIdentityObjectId" +
        '?$select=id,appId,displayName,servicePrincipalType,accountEnabled,agentIdentityBlueprintId'
    Invoke-MgGraphRequest -Method GET -Uri $uri
}

function Get-GatewayApplication {
    $filter = [uri]::EscapeDataString("appId eq '$GatewayAppId'")
    $select = [uri]::EscapeDataString('id,appId,displayName,api,optionalClaims')
    $result = Invoke-MgGraphRequest -Method GET -Uri "https://graph.microsoft.com/v1.0/applications?`$filter=$filter&`$select=$select"
    $apps = @($result.value)
    if ($apps.Count -ne 1) {
        throw "Expected exactly one application for appId $GatewayAppId; found $($apps.Count)."
    }
    $apps[0]
}

function Write-ManifestBackup {
    param([object] $Application)

    $absoluteDirectory = Join-Path (Get-Location) $BackupDirectory
    New-Item -ItemType Directory -Path $absoluteDirectory -Force | Out-Null
    $stamp = Get-Date -Format 'yyyyMMddTHHmmssZ'
    $path = Join-Path $absoluteDirectory "gateway-manifest-$stamp.json"
    $Application | ConvertTo-Json -Depth 32 | Set-Content -LiteralPath $path -Encoding utf8
    $path
}

function Get-OptionalClaimsBody {
    param([object] $OptionalClaims)

    if ($null -eq $OptionalClaims) {
        return @{ accessToken = @(); idToken = @(); saml2Token = @() }
    }
    # Preserve every configured claim and property in every token category.
    @{ 
        accessToken = @($OptionalClaims.accessToken | Where-Object { $null -ne $_ })
        idToken = @($OptionalClaims.idToken | Where-Object { $null -ne $_ })
        saml2Token = @($OptionalClaims.saml2Token | Where-Object { $null -ne $_ })
    }
}

function New-AgentIdentityMinter {
    param([object] $Agent)

    if ([string]::IsNullOrWhiteSpace($BlueprintClientId)) {
        throw 'BlueprintClientId is required with -ScaffoldAgentIdentityMinter.'
    }
    $dir = Join-Path (Get-Location) '.state/s0b-agent-id-minter'
    New-Item -ItemType Directory -Path $dir -Force | Out-Null
    @'
<Project Sdk="Microsoft.NET.Sdk.Web">
  <PropertyGroup>
    <TargetFramework>net8.0</TargetFramework>
    <ImplicitUsings>enable</ImplicitUsings>
    <Nullable>enable</Nullable>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="Microsoft.Identity.Web" Version="4.*" />
    <PackageReference Include="Microsoft.Identity.Web.AgentIdentities" Version="4.*" />
  </ItemGroup>
</Project>
'@ | Set-Content -LiteralPath (Join-Path $dir 'S0bAgentMinter.csproj') -Encoding utf8

    @'
using System.Diagnostics;
using System.Text;
using System.Text.Json;
using Microsoft.Identity.Abstractions;
using Microsoft.Identity.Web;
using Microsoft.Identity.Web.Resource;
using Microsoft.Identity.Web.TokenCacheProviders.InMemory;

static string Required(string name) => Environment.GetEnvironmentVariable(name)
    ?? throw new InvalidOperationException($"{name} is required.");

var values = new Dictionary<string, string?> {
    ["AzureAd:Instance"] = "https://login.microsoftonline.com/",
    ["AzureAd:TenantId"] = Required("EB_S0B_TENANT"),
    ["AzureAd:ClientId"] = Required("EB_S0B_BLUEPRINT_CLIENT_ID"),
    ["AzureAd:ClientCredentials:0:SourceType"] = "ClientSecret",
    ["AzureAd:ClientCredentials:0:ClientSecret"] = Required("EB_S0B_BLUEPRINT_CLIENT_SECRET")
};
var builder = WebApplication.CreateBuilder(args);
builder.Configuration.AddInMemoryCollection(values);
builder.Services.AddMicrosoftIdentityWebApiAuthentication(builder.Configuration);
builder.Services.AddAgentIdentities();
builder.Services.AddInMemoryTokenCaches();
using var app = builder.Build();

var provider = app.Services.GetRequiredService<IAuthorizationHeaderProvider>();
var agentIdentityClientId = Required("EB_S0B_AGENT_IDENTITY_CLIENT_ID");
var options = new AuthorizationHeaderProviderOptions()
    .WithAgentIdentity(agentIdentityClientId);
var header = await provider.CreateAuthorizationHeaderForAppAsync(
    $"api://{Required("EB_S0B_GATEWAY_APP_ID")}/.default", options);
var token = header["Bearer ".Length..];

var payload = token.Split('.')[1].Replace('-', '+').Replace('_', '/');
payload = payload.PadRight(payload.Length + ((4 - payload.Length % 4) % 4), '=');
using var document = JsonDocument.Parse(Convert.FromBase64String(payload));
var root = document.RootElement;
string? StringClaim(string name) => root.TryGetProperty(name, out var value) && value.ValueKind == JsonValueKind.String ? value.GetString() : null;
string[] StringArrayClaim(string name) => root.TryGetProperty(name, out var value) && value.ValueKind == JsonValueKind.Array
    ? value.EnumerateArray().Where(v => v.ValueKind == JsonValueKind.String).Select(v => v.GetString()!).ToArray() : Array.Empty<string>();
var safe = new Dictionary<string, object?> {
    ["aud"] = StringClaim("aud"), ["tid"] = StringClaim("tid"), ["oid"] = StringClaim("oid"),
    ["appid"] = StringClaim("appid"), ["azp"] = StringClaim("azp"), ["idtyp"] = StringClaim("idtyp"),
    ["roles"] = StringArrayClaim("roles"), ["scp"] = StringClaim("scp"),
    ["xms_sub_fct"] = StringClaim("xms_sub_fct"), ["xms_act_fct"] = StringClaim("xms_act_fct"),
    ["xms_par_app_azp"] = StringClaim("xms_par_app_azp"), ["xms_idrel"] = StringClaim("xms_idrel"),
    ["ver"] = StringClaim("ver")
};
Console.WriteLine(JsonSerializer.Serialize(safe)); // Never print token/header.
bool Facet(string name) => (StringClaim(name) ?? "").Split(' ', StringSplitOptions.RemoveEmptyEntries).Contains("11");
var idtyp = StringClaim("idtyp");
var scope = StringClaim("scp");
bool accepted = StringClaim("oid") == agentIdentityClientId
    && StringArrayClaim("roles").Contains("Agent.Access") && Facet("xms_sub_fct") && Facet("xms_act_fct")
    && (idtyp is null or "app") && (string.IsNullOrWhiteSpace(scope) || scope == "/");
if (!accepted) throw new InvalidOperationException("Minted token is not the required autonomous Agent Identity shape.");

var start = new ProcessStartInfo("cargo", "test -p service --test bridge_live -- --ignored --nocapture") {
    UseShellExecute = false, WorkingDirectory = Required("EB_S0B_REPO_ROOT")
};
start.Environment["EB_S0B_TOKEN"] = token;
start.Environment["EB_S0B_TENANT"] = Required("EB_S0B_TENANT");
start.Environment["EB_S0B_AUDIENCE"] = $"api://{Required("EB_S0B_GATEWAY_APP_ID")}";
start.Environment["EB_S0B_OID"] = agentIdentityClientId;
start.Environment["EB_S0B_PRINCIPAL"] = Required("EB_S0B_PRINCIPAL");
start.Environment.Remove("EB_S0B_BLUEPRINT_CLIENT_SECRET");
using var cargo = Process.Start(start) ?? throw new InvalidOperationException("Unable to start cargo.");
await cargo.WaitForExitAsync();
Environment.SetEnvironmentVariable("EB_S0B_BLUEPRINT_CLIENT_SECRET", null);
Environment.SetEnvironmentVariable("EB_S0B_TOKEN", null);
return cargo.ExitCode;
'@ | Set-Content -LiteralPath (Join-Path $dir 'Program.cs') -Encoding utf8NoBOM

    Write-Output "Scaffolded local minter in $dir. It uses Microsoft.Identity.Web.AgentIdentities and does not persist credentials or tokens."
    if ($RunScaffoldedMinter) {
        if ([string]::IsNullOrWhiteSpace($Principal)) {
            throw 'Principal is required with -RunScaffoldedMinter.'
        }
        $secret = Read-Host 'Blueprint client secret (held in memory only)' -AsSecureString
        $ptr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secret)
        try {
            $env:EB_S0B_BLUEPRINT_CLIENT_SECRET = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($ptr)
            $env:EB_S0B_BLUEPRINT_CLIENT_ID = $BlueprintClientId
            $env:EB_S0B_AGENT_IDENTITY_CLIENT_ID = $Agent.appId
            $env:EB_S0B_GATEWAY_APP_ID = $GatewayAppId
            $env:EB_S0B_TENANT = $TenantId
            $env:EB_S0B_PRINCIPAL = $Principal
            $env:EB_S0B_REPO_ROOT = (Get-Location).Path
            & dotnet run --project (Join-Path $dir 'S0bAgentMinter.csproj')
            if ($LASTEXITCODE -ne 0) { throw "Agent Identity minter exited $LASTEXITCODE." }
        }
        finally {
            if ($ptr -ne [IntPtr]::Zero) { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($ptr) }
            Remove-Item Env:EB_S0B_BLUEPRINT_CLIENT_SECRET -ErrorAction SilentlyContinue
            Remove-Item Env:EB_S0B_TOKEN -ErrorAction SilentlyContinue
        }
    }
}

Require-GraphPowerShell
Require-GraphContext
$agent = Get-AgentIdentity
$gateway = Get-GatewayApplication
$backupPath = Write-ManifestBackup $gateway
$accessClaims = Get-OptionalClaimsBody $gateway.optionalClaims
$accessClaimNames = @($accessClaims.accessToken | ForEach-Object { $_.name })

[ordered]@{
    agent_identity_object_id = $agent.id
    agent_identity_client_id = $agent.appId
    agent_identity_display_name = $agent.displayName
    agent_identity_enabled = $agent.accountEnabled
    agent_identity_blueprint_id = $agent.agentIdentityBlueprintId
    genuine_agent_identity = -not [string]::IsNullOrWhiteSpace($agent.agentIdentityBlueprintId)
    gateway_application_object_id = $gateway.id
    gateway_application_client_id = $gateway.appId
    gateway_display_name = $gateway.displayName
    requested_access_token_version = $gateway.api.requestedAccessTokenVersion
    access_token_optional_claims = $accessClaimNames
    requests_idtyp = $accessClaimNames -contains 'idtyp'
    facets_configurable_via_documented_optional_claims = $false
    documented_agent_token_facets = @('xms_sub_fct', 'xms_act_fct', 'xms_par_app_azp')
    manifest_backup = $backupPath
} | ConvertTo-Json -Depth 8

if ($ScaffoldAgentIdentityMinter) { New-AgentIdentityMinter $agent }

