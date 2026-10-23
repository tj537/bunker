# Security

Solarum is an offline music app: no account, no tracking, no server. The only time it goes online is the update check at start (one request to the GitHub releases API, which you can switch off in the settings). It reads and writes `.solarum` project files and WAV recordings that you choose.

## Supported versions

Only the [latest release](https://github.com/tj537/solarum/releases/latest) gets fixes. Please update before reporting.

## Reporting a problem

If you find a security problem – for example a `.solarum` file that crashes Solarum or makes it do something it shouldn't, or a problem with the downloads – please **don't open a public issue**. Report it privately instead:

**[Report a vulnerability](https://github.com/tj537/solarum/security/advisories/new)** (GitHub's private reporting)

Say what you found, how to reproduce it, and which version and system you use. You'll get an answer as soon as possible; Solarum is made by one person in their free time, so please be patient. Once it's fixed, you'll be credited in the release notes if you like.

## Verifying downloads

Every release comes with a `SHA256SUMS.txt`. To check a download on macOS or Linux:

```bash
shasum -a 256 -c SHA256SUMS.txt --ignore-missing
```

On Windows (PowerShell), compare the output of `Get-FileHash Bunker-windows-x64.zip` with the line in `SHA256SUMS.txt`.
