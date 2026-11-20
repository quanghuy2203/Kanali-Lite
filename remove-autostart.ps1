# Go bo muc tu khoi dong cu (ban kanali-rs daemon dung VBS) va tat tien trinh cu.
$vbs = Join-Path ([Environment]::GetFolderPath('Startup')) 'kanali-rs.vbs'
Remove-Item $vbs -ErrorAction SilentlyContinue
Get-Process kanali-rs -ErrorAction SilentlyContinue | Stop-Process -Force
Write-Host "Da go bo muc tu khoi dong cu."
