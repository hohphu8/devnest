use crate::error::AppError;
use crate::utils::process::configure_background_command;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
};
#[cfg(windows)]
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, REG_EXPAND_SZ, REG_SZ};
#[cfg(windows)]
use winreg::{RegKey, RegValue};

pub fn hosts_file_path() -> PathBuf {
    match env::var("DEVNEST_HOSTS_PATH") {
        Ok(value) if !value.trim().is_empty() => PathBuf::from(value),
        _ => PathBuf::from(r"C:\Windows\System32\drivers\etc\hosts"),
    }
}

fn escape_powershell_single_quoted(value: &str) -> String {
    value.replace('\'', "''")
}

fn normalize_env_path_entry(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .trim_end_matches(['\\', '/'])
        .to_ascii_lowercase()
}

pub fn generate_local_ssl_authority(
    cert_pem_path: &Path,
    cert_der_path: &Path,
    key_der_path: &Path,
) -> Result<(), AppError> {
    for path in [cert_pem_path, cert_der_path, key_der_path] {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                AppError::with_details(
                    "SSL_PROVISION_FAILED",
                    "Could not create the managed SSL authority directory.",
                    error.to_string(),
                )
            })?;
        }
    }

    let cert_pem_arg = escape_powershell_single_quoted(&cert_pem_path.to_string_lossy());
    let cert_der_arg = escape_powershell_single_quoted(&cert_der_path.to_string_lossy());
    let key_der_arg = escape_powershell_single_quoted(&key_der_path.to_string_lossy());
    let command = format!(
        "$ErrorActionPreference = 'Stop'; \
         $certPemPath = '{cert_pem_path}'; \
         $certDerPath = '{cert_der_path}'; \
         $keyDerPath = '{key_der_path}'; \
         $caKey = [System.Security.Cryptography.RSA]::Create(2048); \
         $caRequest = [System.Security.Cryptography.X509Certificates.CertificateRequest]::new('CN=DevNest Local CA', $caKey, [System.Security.Cryptography.HashAlgorithmName]::SHA256, [System.Security.Cryptography.RSASignaturePadding]::Pkcs1); \
         $caRequest.CertificateExtensions.Add([System.Security.Cryptography.X509Certificates.X509BasicConstraintsExtension]::new($true, $false, 0, $true)); \
         $caRequest.CertificateExtensions.Add([System.Security.Cryptography.X509Certificates.X509SubjectKeyIdentifierExtension]::new($caRequest.PublicKey, $false)); \
         $caKeyUsage = [System.Security.Cryptography.X509Certificates.X509KeyUsageFlags]::KeyCertSign -bor [System.Security.Cryptography.X509Certificates.X509KeyUsageFlags]::CrlSign -bor [System.Security.Cryptography.X509Certificates.X509KeyUsageFlags]::DigitalSignature; \
         $caRequest.CertificateExtensions.Add([System.Security.Cryptography.X509Certificates.X509KeyUsageExtension]::new($caKeyUsage, $true)); \
         $caCertificate = $caRequest.CreateSelfSigned([DateTimeOffset]::UtcNow.AddDays(-1), [DateTimeOffset]::UtcNow.AddYears(10)); \
         $caCertDer = $caCertificate.Export([System.Security.Cryptography.X509Certificates.X509ContentType]::Cert); \
         [System.IO.File]::WriteAllBytes($certDerPath, $caCertDer); \
         $caCertPemBody = [Convert]::ToBase64String($caCertDer, 'InsertLineBreaks'); \
         $caCertPem = \"-----BEGIN CERTIFICATE-----`n$caCertPemBody`n-----END CERTIFICATE-----`n\"; \
         [System.IO.File]::WriteAllText($certPemPath, $caCertPem, [System.Text.UTF8Encoding]::new($false)); \
         $caKeyDer = ([System.Security.Cryptography.RSACng]$caKey).Key.Export([System.Security.Cryptography.CngKeyBlobFormat]::Pkcs8PrivateBlob); \
         [System.IO.File]::WriteAllBytes($keyDerPath, $caKeyDer);",
        cert_pem_path = cert_pem_arg,
        cert_der_path = cert_der_arg,
        key_der_path = key_der_arg,
    );

    let mut process = Command::new("powershell");
    process.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &command,
    ]);
    configure_background_command(&mut process);
    let output = process.output().map_err(|error| {
        AppError::with_details(
            "SSL_PROVISION_FAILED",
            "Could not start the local SSL authority helper.",
            error.to_string(),
        )
    })?;

    if output.status.success()
        && cert_pem_path.exists()
        && cert_der_path.exists()
        && key_der_path.exists()
    {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let details = [stderr.as_str(), stdout.as_str()]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    Err(AppError::with_details(
        "SSL_PROVISION_FAILED",
        "Could not generate the local DevNest certificate authority.",
        if details.is_empty() {
            "The certificate authority helper returned a non-zero exit code.".to_string()
        } else {
            details
        },
    ))
}

pub fn generate_signed_certificate_pem(
    domain: &str,
    issuer_cert_der_path: &Path,
    issuer_key_der_path: &Path,
    cert_path: &Path,
    key_path: &Path,
) -> Result<(), AppError> {
    for path in [cert_path, key_path] {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                AppError::with_details(
                    "SSL_PROVISION_FAILED",
                    "Could not create the managed SSL project directory.",
                    error.to_string(),
                )
            })?;
        }
    }

    let domain_arg = escape_powershell_single_quoted(domain);
    let issuer_cert_arg = escape_powershell_single_quoted(&issuer_cert_der_path.to_string_lossy());
    let issuer_key_arg = escape_powershell_single_quoted(&issuer_key_der_path.to_string_lossy());
    let cert_arg = escape_powershell_single_quoted(&cert_path.to_string_lossy());
    let key_arg = escape_powershell_single_quoted(&key_path.to_string_lossy());
    let command = format!(
        "$ErrorActionPreference = 'Stop'; \
         $domain = '{domain}'; \
         $issuerCertPath = '{issuer_cert_path}'; \
         $issuerKeyPath = '{issuer_key_path}'; \
         $certPath = '{cert_path}'; \
         $keyPath = '{key_path}'; \
         $issuerCertificate = [System.Security.Cryptography.X509Certificates.X509Certificate2]::new([System.IO.File]::ReadAllBytes($issuerCertPath)); \
         $issuerKey = [System.Security.Cryptography.RSACng]::new([System.Security.Cryptography.CngKey]::Import([System.IO.File]::ReadAllBytes($issuerKeyPath), [System.Security.Cryptography.CngKeyBlobFormat]::Pkcs8PrivateBlob)); \
         $signatureGenerator = [System.Security.Cryptography.X509Certificates.X509SignatureGenerator]::CreateForRSA($issuerKey, [System.Security.Cryptography.RSASignaturePadding]::Pkcs1); \
         $leafKey = [System.Security.Cryptography.RSA]::Create(2048); \
         $leafRequest = [System.Security.Cryptography.X509Certificates.CertificateRequest]::new(\"CN=$domain\", $leafKey, [System.Security.Cryptography.HashAlgorithmName]::SHA256, [System.Security.Cryptography.RSASignaturePadding]::Pkcs1); \
         $sanBuilder = [System.Security.Cryptography.X509Certificates.SubjectAlternativeNameBuilder]::new(); \
         $sanBuilder.AddDnsName($domain); \
         $leafRequest.CertificateExtensions.Add($sanBuilder.Build()); \
         $leafRequest.CertificateExtensions.Add([System.Security.Cryptography.X509Certificates.X509BasicConstraintsExtension]::new($false, $false, 0, $false)); \
         $leafRequest.CertificateExtensions.Add([System.Security.Cryptography.X509Certificates.X509SubjectKeyIdentifierExtension]::new($leafRequest.PublicKey, $false)); \
         $eku = [System.Security.Cryptography.OidCollection]::new(); \
         $null = $eku.Add([System.Security.Cryptography.Oid]::new('1.3.6.1.5.5.7.3.1')); \
         $leafRequest.CertificateExtensions.Add([System.Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension]::new($eku, $false)); \
         $leafKeyUsage = [System.Security.Cryptography.X509Certificates.X509KeyUsageFlags]::DigitalSignature -bor [System.Security.Cryptography.X509Certificates.X509KeyUsageFlags]::KeyEncipherment; \
         $leafRequest.CertificateExtensions.Add([System.Security.Cryptography.X509Certificates.X509KeyUsageExtension]::new($leafKeyUsage, $true)); \
         $rng = [System.Security.Cryptography.RandomNumberGenerator]::Create(); \
         $serial = New-Object byte[] 16; \
         $rng.GetBytes($serial); \
         $leafCertificate = $leafRequest.Create($issuerCertificate.SubjectName, $signatureGenerator, [DateTimeOffset]::UtcNow.AddDays(-1), [DateTimeOffset]::UtcNow.AddYears(2), $serial); \
         $leafCertDer = $leafCertificate.Export([System.Security.Cryptography.X509Certificates.X509ContentType]::Cert); \
         $leafCertPemBody = [Convert]::ToBase64String($leafCertDer, 'InsertLineBreaks'); \
         $leafCertPem = \"-----BEGIN CERTIFICATE-----`n$leafCertPemBody`n-----END CERTIFICATE-----`n\"; \
         [System.IO.File]::WriteAllText($certPath, $leafCertPem, [System.Text.UTF8Encoding]::new($false)); \
         $leafKeyDer = ([System.Security.Cryptography.RSACng]$leafKey).Key.Export([System.Security.Cryptography.CngKeyBlobFormat]::Pkcs8PrivateBlob); \
         $leafKeyPemBody = [Convert]::ToBase64String($leafKeyDer, 'InsertLineBreaks'); \
         $leafKeyPem = \"-----BEGIN PRIVATE KEY-----`n$leafKeyPemBody`n-----END PRIVATE KEY-----`n\"; \
         [System.IO.File]::WriteAllText($keyPath, $leafKeyPem, [System.Text.UTF8Encoding]::new($false));",
        domain = domain_arg,
        issuer_cert_path = issuer_cert_arg,
        issuer_key_path = issuer_key_arg,
        cert_path = cert_arg,
        key_path = key_arg,
    );

    let mut process = Command::new("powershell");
    process.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &command,
    ]);
    configure_background_command(&mut process);
    let output = process.output().map_err(|error| {
        AppError::with_details(
            "SSL_PROVISION_FAILED",
            "Could not start the local SSL leaf certificate helper.",
            error.to_string(),
        )
    })?;

    if output.status.success() && cert_path.exists() && key_path.exists() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let details = [stderr.as_str(), stdout.as_str()]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    Err(AppError::with_details(
        "SSL_PROVISION_FAILED",
        "Could not generate the project SSL certificate files.",
        if details.is_empty() {
            "The project certificate helper returned a non-zero exit code.".to_string()
        } else {
            details
        },
    ))
}

pub fn trust_certificate_for_current_user(cert_path: &Path) -> Result<(), AppError> {
    if !cert_path.exists() {
        return Err(AppError::new_validation(
            "SSL_CERTIFICATE_NOT_FOUND",
            "The local SSL certificate file does not exist yet.",
        ));
    }

    let escaped_cert_path = escape_powershell_single_quoted(&cert_path.to_string_lossy());
    let command = format!(
        "$ErrorActionPreference = 'Stop'; \
         $certPath = '{cert_path}'; \
         $certificate = [System.Security.Cryptography.X509Certificates.X509Certificate2]::new($certPath); \
         $store = [System.Security.Cryptography.X509Certificates.X509Store]::new('Root', 'CurrentUser'); \
         $store.Open([System.Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite); \
         try {{ \
             $existing = $store.Certificates | Where-Object {{ $_.Thumbprint -eq $certificate.Thumbprint }}; \
             if (-not $existing) {{ $store.Add($certificate) }} \
         }} finally {{ \
             $store.Close() \
         }}",
        cert_path = escaped_cert_path,
    );

    let mut process = Command::new("powershell");
    process.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &command,
    ]);
    configure_background_command(&mut process);
    let output = process.output().map_err(|error| {
        AppError::with_details(
            "SSL_TRUST_FAILED",
            "Could not start the local certificate trust helper.",
            error.to_string(),
        )
    })?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let details = [stderr.as_str(), stdout.as_str()]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    Err(AppError::with_details(
        "SSL_TRUST_FAILED",
        "Could not trust the local SSL certificate in the current user store.",
        if details.is_empty() {
            "The trust helper returned a non-zero exit code.".to_string()
        } else {
            details
        },
    ))
}

pub fn is_certificate_trusted_for_current_user(cert_path: &Path) -> Result<bool, AppError> {
    if !cert_path.exists() {
        return Ok(false);
    }

    let escaped_cert_path = escape_powershell_single_quoted(&cert_path.to_string_lossy());
    let command = format!(
        "$ErrorActionPreference = 'Stop'; \
         $certPath = '{cert_path}'; \
         $certificate = [System.Security.Cryptography.X509Certificates.X509Certificate2]::new($certPath); \
         $store = [System.Security.Cryptography.X509Certificates.X509Store]::new('Root', 'CurrentUser'); \
         $store.Open([System.Security.Cryptography.X509Certificates.OpenFlags]::ReadOnly); \
         try {{ \
             $existing = $store.Certificates | Where-Object {{ $_.Thumbprint -eq $certificate.Thumbprint }}; \
             if ($existing) {{ 'trusted' }} else {{ 'missing' }} \
         }} finally {{ \
             $store.Close() \
         }}",
        cert_path = escaped_cert_path,
    );

    let mut process = Command::new("powershell");
    process.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &command,
    ]);
    configure_background_command(&mut process);
    let output = process.output().map_err(|error| {
        AppError::with_details(
            "SSL_TRUST_STATUS_FAILED",
            "Could not inspect the current user trust store.",
            error.to_string(),
        )
    })?;

    if !output.status.success() {
        return Err(AppError::with_details(
            "SSL_TRUST_STATUS_FAILED",
            "Could not inspect whether the local SSL certificate is trusted.",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).contains("trusted"))
}

pub fn remove_certificate_from_current_user_store(cert_path: &Path) -> Result<bool, AppError> {
    if !cert_path.exists() {
        return Ok(false);
    }

    let escaped_cert_path = escape_powershell_single_quoted(&cert_path.to_string_lossy());
    let command = format!(
        "$ErrorActionPreference = 'Stop'; \
         $certPath = '{cert_path}'; \
         $certificate = [System.Security.Cryptography.X509Certificates.X509Certificate2]::new($certPath); \
         $store = [System.Security.Cryptography.X509Certificates.X509Store]::new('Root', 'CurrentUser'); \
         $store.Open([System.Security.Cryptography.X509Certificates.OpenFlags]::ReadWrite); \
         try {{ \
             $existing = @($store.Certificates | Where-Object {{ $_.Thumbprint -eq $certificate.Thumbprint }}); \
             if ($existing.Count -gt 0) {{ \
                 foreach ($item in $existing) {{ $store.Remove($item) }}; \
                 'removed' \
             }} else {{ \
                 'missing' \
             }} \
         }} finally {{ \
             $store.Close() \
         }}",
        cert_path = escaped_cert_path,
    );

    let mut process = Command::new("powershell");
    process.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &command,
    ]);
    configure_background_command(&mut process);
    let output = process.output().map_err(|error| {
        AppError::with_details(
            "SSL_UNTRUST_FAILED",
            "Could not start the local certificate untrust helper.",
            error.to_string(),
        )
    })?;

    if !output.status.success() {
        return Err(AppError::with_details(
            "SSL_UNTRUST_FAILED",
            "Could not remove the DevNest local certificate authority from the current user store.",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).contains("removed"))
}

fn elevated_hosts_script() -> &'static str {
    r#"
param(
    [Parameter(Mandatory = $true)][string]$Operation,
    [Parameter(Mandatory = $true)][string]$HostsPath,
    [Parameter(Mandatory = $true)][string]$Domain,
    [string]$TargetIp = "127.0.0.1"
)

$ErrorActionPreference = "Stop"

function Normalize-Domain {
    param([string]$Value)
    return $Value.Trim().ToLowerInvariant()
}

function Split-Line {
    param([string]$Line)

    $index = $Line.IndexOf('#')
    if ($index -lt 0) {
        return @($Line, $null)
    }

    $body = $Line.Substring(0, $index)
    $comment = $Line.Substring($index).Trim()
    return @($body, $comment)
}

function Render-Entry {
    param(
        [string]$Ip,
        [System.Collections.Generic.List[string]]$Domains,
        [string]$Comment
    )

    $line = "$Ip`t$($Domains -join ' ')"
    if (-not [string]::IsNullOrWhiteSpace($Comment)) {
        $line = "$line $Comment"
    }

    return $line
}

function Unique-Domains {
    param([string[]]$Domains)

    $seen = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
    $ordered = New-Object 'System.Collections.Generic.List[string]'

    foreach ($domain in $Domains) {
        $normalized = Normalize-Domain $domain
        if (-not [string]::IsNullOrWhiteSpace($normalized) -and $seen.Add($normalized)) {
            [void]$ordered.Add($normalized)
        }
    }

    return $ordered
}

$normalizedDomain = Normalize-Domain $Domain
$content = if (Test-Path -LiteralPath $HostsPath) {
    Get-Content -LiteralPath $HostsPath -Raw -ErrorAction Stop
} else {
    ""
}

$lines = if ([string]::IsNullOrEmpty($content)) {
    @()
} else {
    [regex]::Split($content, "`r?`n")
}

$updated = New-Object 'System.Collections.Generic.List[string]'
$keptTargetEntry = $false

foreach ($rawLine in $lines) {
    $trimmed = $rawLine.Trim()
    if ([string]::IsNullOrWhiteSpace($trimmed) -or $trimmed.StartsWith('#')) {
        [void]$updated.Add($rawLine)
        continue
    }

    $parts = Split-Line $rawLine
    $body = $parts[0].Trim()
    $comment = $parts[1]
    $tokens = @($body -split '\s+' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })

    if ($tokens.Count -lt 2) {
        [void]$updated.Add($rawLine)
        continue
    }

    $ip = $tokens[0]
    $domains = Unique-Domains ($tokens[1..($tokens.Count - 1)])

    if ($domains.Contains($normalizedDomain)) {
        if ($Operation -eq 'apply' -and $ip -eq $TargetIp -and -not $keptTargetEntry) {
            [void]$updated.Add((Render-Entry -Ip $ip -Domains $domains -Comment $comment))
            $keptTargetEntry = $true
            continue
        }

        $remaining = New-Object 'System.Collections.Generic.List[string]'
        foreach ($domain in $domains) {
            if ($domain -ne $normalizedDomain) {
                [void]$remaining.Add($domain)
            }
        }

        if ($remaining.Count -gt 0) {
            [void]$updated.Add((Render-Entry -Ip $ip -Domains $remaining -Comment $comment))
        }

        continue
    }

    [void]$updated.Add($rawLine)
}

if ($Operation -eq 'apply' -and -not $keptTargetEntry) {
    if ($updated.Count -gt 0 -and -not [string]::IsNullOrWhiteSpace($updated[$updated.Count - 1])) {
        [void]$updated.Add("")
    }

    [void]$updated.Add("$TargetIp`t$normalizedDomain # devnest")
}

$rendered = if ($updated.Count -eq 0) {
    ""
} else {
    ($updated -join "`r`n").TrimEnd("`r", "`n") + "`r`n"
}

[System.IO.File]::WriteAllText($HostsPath, $rendered, [System.Text.Encoding]::ASCII)
exit 0
"#
}

fn write_elevated_hosts_script(script_path: &Path) -> Result<(), AppError> {
    fs::write(script_path, elevated_hosts_script()).map_err(|error| {
        AppError::with_details(
            "HOSTS_ELEVATION_FAILED",
            "Could not prepare the elevated hosts helper script.",
            error.to_string(),
        )
    })
}

fn run_elevated_hosts_helper(
    operation: &str,
    hosts_path: &Path,
    domain: &str,
    target_ip: Option<&str>,
) -> Result<(), AppError> {
    let script_path = env::temp_dir().join(format!("devnest-hosts-helper-{}.ps1", Uuid::new_v4()));
    write_elevated_hosts_script(&script_path)?;

    let script_arg = escape_powershell_single_quoted(&script_path.to_string_lossy());
    let hosts_arg = escape_powershell_single_quoted(&hosts_path.to_string_lossy());
    let domain_arg = escape_powershell_single_quoted(domain);
    let target_arg = escape_powershell_single_quoted(target_ip.unwrap_or("127.0.0.1"));
    let outer_command = format!(
        "try {{ \
            $process = Start-Process -FilePath 'powershell.exe' -Verb RunAs -Wait -PassThru -WindowStyle Hidden -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File','{script}','-Operation','{operation}','-HostsPath','{hosts}','-Domain','{domain}','-TargetIp','{target}'); \
            exit $process.ExitCode \
        }} catch {{ \
            Write-Error $_; \
            exit 1223 \
        }}",
        script = script_arg,
        operation = operation,
        hosts = hosts_arg,
        domain = domain_arg,
        target = target_arg,
    );

    let mut process = Command::new("powershell");
    process.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &outer_command,
    ]);
    configure_background_command(&mut process);
    let output = process.output().map_err(|error| {
        AppError::with_details(
            "HOSTS_ELEVATION_FAILED",
            "Could not request administrator permission to update local domains.",
            error.to_string(),
        )
    })?;

    fs::remove_file(&script_path).ok();

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let combined = [stderr.as_str(), stdout.as_str()]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let lowered = combined.to_ascii_lowercase();

    if lowered.contains("canceled")
        || lowered.contains("cancelled")
        || lowered.contains("1223")
        || lowered.contains("operation was canceled")
    {
        return Err(AppError::new_validation(
            "HOSTS_PERMISSION_DENIED",
            "Administrator permission is still required to update the Windows hosts file.",
        ));
    }

    Err(AppError::with_details(
        "HOSTS_ELEVATION_FAILED",
        "Administrator permission was granted, but the elevated hosts update did not complete.",
        if combined.is_empty() {
            "The elevated helper returned a non-zero exit code.".to_string()
        } else {
            combined
        },
    ))
}

pub fn apply_hosts_file_with_elevation(
    hosts_path: &Path,
    domain: &str,
    target_ip: &str,
) -> Result<(), AppError> {
    run_elevated_hosts_helper("apply", hosts_path, domain, Some(target_ip))
}

pub fn remove_hosts_file_with_elevation(hosts_path: &Path, domain: &str) -> Result<(), AppError> {
    run_elevated_hosts_helper("remove", hosts_path, domain, None)
}

pub fn reveal_in_explorer(target_path: &Path) -> Result<(), AppError> {
    if target_path.exists() {
        let explorer_arg = format!("/select,{}", target_path.to_string_lossy());
        Command::new("explorer")
            .arg(explorer_arg)
            .spawn()
            .map_err(|error| {
                AppError::with_details(
                    "PATH_REVEAL_FAILED",
                    "DevNest could not open Windows Explorer for the selected runtime path.",
                    error.to_string(),
                )
            })?;

        return Ok(());
    }

    let parent = target_path.parent().ok_or_else(|| {
        AppError::new_validation(
            "PATH_REVEAL_FAILED",
            "DevNest could not determine which folder to open for this runtime path.",
        )
    })?;

    if !parent.exists() {
        return Err(AppError::new_validation(
            "PATH_REVEAL_FAILED",
            "The runtime path and its parent folder are both missing, so there is nothing to reveal.",
        ));
    }

    Command::new("explorer")
        .arg(parent)
        .spawn()
        .map_err(|error| {
            AppError::with_details(
                "PATH_REVEAL_FAILED",
                "DevNest could not open Windows Explorer for the selected runtime folder.",
                error.to_string(),
            )
        })?;

    Ok(())
}

pub fn open_file_in_default_app(path: &Path) -> Result<(), AppError> {
    if !path.exists() || !path.is_file() {
        return Err(AppError::new_validation(
            "RUNTIME_CONFIG_OPEN_FAILED",
            "The managed config file does not exist yet.",
        ));
    }

    let escaped_path = escape_powershell_single_quoted(&path.to_string_lossy());
    let command = format!(
        "$ErrorActionPreference = 'Stop'; Start-Process -FilePath '{target_path}' | Out-Null",
        target_path = escaped_path,
    );

    let mut process = Command::new("powershell");
    process.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &command,
    ]);
    configure_background_command(&mut process);
    let output = process.output().map_err(|error| {
        AppError::with_details(
            "RUNTIME_CONFIG_OPEN_FAILED",
            "DevNest could not open the managed config file in the default Windows app.",
            error.to_string(),
        )
    })?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let details = [stderr.as_str(), stdout.as_str()]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    Err(AppError::with_details(
        "RUNTIME_CONFIG_OPEN_FAILED",
        "DevNest could not open the managed config file in the default Windows app.",
        if details.is_empty() {
            "The Windows file opener returned a non-zero exit code.".to_string()
        } else {
            details
        },
    ))
}

#[cfg(windows)]
fn decode_registry_string(value: &RegValue) -> String {
    let utf16 = value
        .bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .take_while(|unit| *unit != 0)
        .collect::<Vec<_>>();

    String::from_utf16_lossy(&utf16)
}

#[cfg(windows)]
fn encode_registry_string(value: &str) -> Vec<u8> {
    value
        .encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(|unit| unit.to_le_bytes())
        .collect()
}

#[cfg(windows)]
pub fn read_user_environment_variable(name: &str) -> Result<Option<String>, AppError> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let environment = match hkcu.open_subkey_with_flags("Environment", KEY_READ) {
        Ok(key) => key,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(AppError::with_details(
                "USER_ENV_READ_FAILED",
                "DevNest could not read the current user environment settings.",
                error.to_string(),
            ));
        }
    };

    match environment.get_raw_value(name) {
        Ok(value) => Ok(Some(decode_registry_string(&value))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AppError::with_details(
            "USER_ENV_READ_FAILED",
            "DevNest could not read the current user environment settings.",
            error.to_string(),
        )),
    }
}

#[cfg(not(windows))]
pub fn read_user_environment_variable(_name: &str) -> Result<Option<String>, AppError> {
    Ok(None)
}

#[cfg(windows)]
pub fn read_system_environment_variable(name: &str) -> Result<Option<String>, AppError> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let environment = match hklm.open_subkey_with_flags(
        "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment",
        KEY_READ,
    ) {
        Ok(key) => key,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(AppError::with_details(
                "SYSTEM_ENV_READ_FAILED",
                "DevNest could not read the system environment settings.",
                error.to_string(),
            ));
        }
    };

    match environment.get_raw_value(name) {
        Ok(value) => Ok(Some(decode_registry_string(&value))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AppError::with_details(
            "SYSTEM_ENV_READ_FAILED",
            "DevNest could not read the system environment settings.",
            error.to_string(),
        )),
    }
}

#[cfg(not(windows))]
pub fn read_system_environment_variable(_name: &str) -> Result<Option<String>, AppError> {
    Ok(None)
}

#[cfg(windows)]
pub fn write_user_environment_variable(name: &str, value: Option<&str>) -> Result<(), AppError> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (environment, _) = hkcu.create_subkey("Environment").map_err(|error| {
        AppError::with_details(
            "USER_ENV_WRITE_FAILED",
            "DevNest could not update the current user environment settings.",
            error.to_string(),
        )
    })?;

    match value {
        Some(value) => environment
            .set_raw_value(
                name,
                &RegValue {
                    bytes: encode_registry_string(value),
                    vtype: if value.contains('%') {
                        REG_EXPAND_SZ
                    } else {
                        REG_SZ
                    },
                },
            )
            .map_err(|error| {
                AppError::with_details(
                    "USER_ENV_WRITE_FAILED",
                    "DevNest could not update the current user environment settings.",
                    error.to_string(),
                )
            })?,
        None => {
            let _ = environment.delete_value(name);
        }
    }

    Ok(())
}

#[cfg(not(windows))]
pub fn write_user_environment_variable(_name: &str, _value: Option<&str>) -> Result<(), AppError> {
    Ok(())
}

#[cfg(windows)]
pub fn broadcast_environment_change() -> Result<(), AppError> {
    let wide_message = "Environment\0".encode_utf16().collect::<Vec<_>>();
    let mut result = 0usize;
    let send_status = unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            wide_message.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            5000,
            &mut result,
        )
    };

    if send_status == 0 {
        return Err(AppError::new_validation(
            "USER_ENV_NOTIFY_FAILED",
            "DevNest updated the user environment but could not notify Windows about the change.",
        ));
    }

    Ok(())
}

#[cfg(not(windows))]
pub fn broadcast_environment_change() -> Result<(), AppError> {
    Ok(())
}

#[cfg(windows)]
fn elevated_system_path_script() -> &'static str {
    r#"
param(
    [Parameter(Mandatory = $true)][string]$TargetDir
)

$ErrorActionPreference = "Stop"
$regPath = "Registry::HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Session Manager\Environment"

function Normalize-PathEntry {
    param([string]$Value)
    return $Value.Trim().Trim('"').TrimEnd('\', '/').ToLowerInvariant()
}

$current = (Get-ItemProperty -Path $regPath -Name Path -ErrorAction SilentlyContinue).Path
$entries = New-Object 'System.Collections.Generic.List[string]'
$entries.Add($TargetDir) | Out-Null
$normalizedTarget = Normalize-PathEntry $TargetDir

foreach ($entry in ($current -split ';')) {
    $trimmed = $entry.Trim()
    if ([string]::IsNullOrWhiteSpace($trimmed)) {
        continue
    }

    if ((Normalize-PathEntry $trimmed) -eq $normalizedTarget) {
        continue
    }

    $entries.Add($trimmed) | Out-Null
}

$nextPath = ($entries -join ';')
Remove-ItemProperty -Path $regPath -Name Path -ErrorAction SilentlyContinue
New-ItemProperty -Path $regPath -Name Path -Value $nextPath -PropertyType ExpandString -Force | Out-Null
exit 0
"#
}

#[cfg(windows)]
fn write_elevated_system_path_script(script_path: &Path) -> Result<(), AppError> {
    fs::write(script_path, elevated_system_path_script()).map_err(|error| {
        AppError::with_details(
            "SYSTEM_ENV_WRITE_FAILED",
            "DevNest could not prepare the elevated system PATH helper.",
            error.to_string(),
        )
    })
}

#[cfg(windows)]
pub fn ensure_system_path_dir_with_elevation(target_dir: &Path) -> Result<bool, AppError> {
    let current_system_path = read_system_environment_variable("Path")?.unwrap_or_default();
    let target_dir_string = target_dir.to_string_lossy().to_string();
    if current_system_path.split(';').any(|entry| {
        normalize_env_path_entry(entry) == normalize_env_path_entry(&target_dir_string)
    }) {
        return Ok(false);
    }

    let script_path =
        env::temp_dir().join(format!("devnest-system-path-helper-{}.ps1", Uuid::new_v4()));
    write_elevated_system_path_script(&script_path)?;

    let script_arg = escape_powershell_single_quoted(&script_path.to_string_lossy());
    let target_arg = escape_powershell_single_quoted(&target_dir_string);
    let outer_command = format!(
        "try {{ \
            $process = Start-Process -FilePath 'powershell.exe' -Verb RunAs -Wait -PassThru -WindowStyle Hidden -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File','{script}','-TargetDir','{target}'); \
            exit $process.ExitCode \
        }} catch {{ \
            Write-Error $_; \
            exit 1223 \
        }}",
        script = script_arg,
        target = target_arg,
    );

    let mut process = Command::new("powershell");
    process.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &outer_command,
    ]);
    configure_background_command(&mut process);
    let output = process.output().map_err(|error| {
        AppError::with_details(
            "SYSTEM_ENV_WRITE_FAILED",
            "DevNest could not request administrator permission to update the system PATH.",
            error.to_string(),
        )
    })?;

    fs::remove_file(&script_path).ok();

    if output.status.success() {
        return Ok(true);
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let combined = [stderr.as_str(), stdout.as_str()]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let lowered = combined.to_ascii_lowercase();

    if lowered.contains("canceled")
        || lowered.contains("cancelled")
        || lowered.contains("1223")
        || lowered.contains("operation was canceled")
    {
        return Err(AppError::new_validation(
            "SYSTEM_ENV_PERMISSION_DENIED",
            "Administrator permission was denied, so DevNest could not add its PHP CLI shim to the system PATH.",
        ));
    }

    Err(AppError::with_details(
        "SYSTEM_ENV_WRITE_FAILED",
        "Administrator permission was granted, but DevNest could not update the system PATH.",
        if combined.is_empty() {
            "The elevated system PATH helper returned a non-zero exit code.".to_string()
        } else {
            combined
        },
    ))
}

#[cfg(not(windows))]
pub fn ensure_system_path_dir_with_elevation(_target_dir: &Path) -> Result<bool, AppError> {
    Ok(false)
}

pub fn open_url_in_default_browser(url: &str) -> Result<(), AppError> {
    Command::new("explorer").arg(url).spawn().map_err(|error| {
        AppError::with_details(
            "OPEN_BROWSER_FAILED",
            "DevNest could not open the project URL in the default browser.",
            error.to_string(),
        )
    })?;

    Ok(())
}

pub fn open_folder_in_explorer(path: &Path) -> Result<(), AppError> {
    if !path.exists() || !path.is_dir() {
        return Err(AppError::new_validation(
            "PROJECT_PATH_NOT_FOUND",
            "The project folder does not exist anymore.",
        ));
    }

    Command::new("explorer")
        .arg(path)
        .spawn()
        .map_err(|error| {
            AppError::with_details(
                "OPEN_FOLDER_FAILED",
                "DevNest could not open the project folder in Windows Explorer.",
                error.to_string(),
            )
        })?;

    Ok(())
}

pub fn open_terminal_at_path(path: &Path) -> Result<(), AppError> {
    if !path.exists() || !path.is_dir() {
        return Err(AppError::new_validation(
            "PROJECT_PATH_NOT_FOUND",
            "The project folder does not exist anymore.",
        ));
    }

    let escaped_path = escape_powershell_single_quoted(&path.to_string_lossy());
    let command = format!(
        "$ErrorActionPreference = 'Stop'; \
         $projectPath = '{project_path}'; \
         if (Get-Command wt.exe -ErrorAction SilentlyContinue) {{ \
             Start-Process -FilePath 'wt.exe' -WorkingDirectory $projectPath -ArgumentList @('-d', $projectPath) | Out-Null \
         }} else {{ \
             Start-Process -FilePath 'powershell.exe' -WorkingDirectory $projectPath -ArgumentList @('-NoExit','-NoLogo') | Out-Null \
         }}",
        project_path = escaped_path,
    );

    let mut process = Command::new("powershell");
    process.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &command,
    ]);
    configure_background_command(&mut process);
    let output = process.output().map_err(|error| {
        AppError::with_details(
            "OPEN_TERMINAL_FAILED",
            "DevNest could not launch a terminal process for this project folder.",
            error.to_string(),
        )
    })?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let details = [stderr.as_str(), stdout.as_str()]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    Err(AppError::with_details(
        "OPEN_TERMINAL_FAILED",
        "DevNest could not open a terminal in the project folder.",
        if details.is_empty() {
            "The terminal launcher returned a non-zero exit code.".to_string()
        } else {
            details
        },
    ))
}

fn vscode_binary_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(local_app_data) = env::var("LOCALAPPDATA") {
        candidates.push(
            PathBuf::from(local_app_data)
                .join("Programs")
                .join("Microsoft VS Code")
                .join("Code.exe"),
        );
    }

    candidates.push(PathBuf::from(
        r"C:\Program Files\Microsoft VS Code\Code.exe",
    ));
    candidates.push(PathBuf::from(
        r"C:\Program Files (x86)\Microsoft VS Code\Code.exe",
    ));

    candidates
}

pub fn open_in_vscode(path: &Path) -> Result<(), AppError> {
    if !path.exists() || !path.is_dir() {
        return Err(AppError::new_validation(
            "PROJECT_PATH_NOT_FOUND",
            "The project folder does not exist anymore.",
        ));
    }

    let direct = Command::new("code").arg(path).spawn();
    match direct {
        Ok(_) => return Ok(()),
        Err(_) => {}
    }

    for candidate in vscode_binary_candidates() {
        if candidate.exists() {
            Command::new(&candidate)
                .arg(path)
                .spawn()
                .map_err(|error| {
                    AppError::with_details(
                        "OPEN_VSCODE_FAILED",
                        "DevNest found VS Code but could not open the project there.",
                        error.to_string(),
                    )
                })?;

            return Ok(());
        }
    }

    Err(AppError::new_validation(
        "VSCODE_NOT_FOUND",
        "VS Code was not found on this machine. Install it or add `code` to PATH first.",
    ))
}
