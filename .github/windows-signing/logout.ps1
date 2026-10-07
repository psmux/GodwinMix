# Ends the SimplySign session at the end of a job. Stopping SimplySign
# Desktop takes the virtual smart card away, and with it the certificate in
# CurrentUser\My. The runner is thrown away afterwards anyway; this is so no
# later step of the same job can sign anything.
Get-Process | Where-Object { $_.ProcessName -like '*SimplySign*' } | ForEach-Object {
    Write-Host "stopping $($_.ProcessName) ($($_.Id))"
    Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
}
Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Issuer -like '*Certum*' } |
    Remove-Item -ErrorAction SilentlyContinue
exit 0
