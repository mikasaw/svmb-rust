@echo off
rem host-side UDP listener on 50000, prints one received packet then exits (20s timeout)
powershell -NoProfile -Command "$u=New-Object System.Net.Sockets.UdpClient(50000); $u.Client.ReceiveTimeout=20000; try { $ep=New-Object System.Net.IPEndPoint ([System.Net.IPAddress]::Any,0); $r=$u.Receive([ref]$ep); Write-Output ('RECV '+[Text.Encoding]::ASCII.GetString($r)+' FROM '+$ep.Address) } catch { Write-Output 'TIMEOUT-NOPACKET' }; $u.Close()"
