[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments)]
    [string[]]$Values
)

$Values | ConvertTo-Json -Compress
