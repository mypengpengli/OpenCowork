param([string]$CaptureExecutable=(Join-Path $env:LOCALAPPDATA 'OpenClaw\target\debug\opencowork-shell.exe'))
$ErrorActionPreference='Stop'
$CaptureExecutable=[IO.Path]::GetFullPath($CaptureExecutable)
$testRoot=Join-Path $env:TEMP ('cowork-window-'+[Guid]::NewGuid().ToString('N'))
[void](New-Item -ItemType Directory -Path $testRoot)
$fixturePath=Join-Path $testRoot 'fixture.ps1'
@'
param([string]$Root)
Add-Type -AssemblyName PresentationFramework
Add-Type -AssemblyName PresentationCore
$window=[Windows.Window]::new();$window.Title='OpenCowork window parity fixture';$window.Width=640;$window.Height=500;$window.Topmost=$true
$window.Add_PreviewMouseDown({param($sender,$e);[IO.File]::AppendAllText((Join-Path $Root 'pointer.txt'),('down '+$e.ChangedButton+"`n"))})
$window.Add_PreviewMouseUp({param($sender,$e);[IO.File]::AppendAllText((Join-Path $Root 'pointer.txt'),('up '+$e.ChangedButton+"`n"))})
$panel=[Windows.Controls.StackPanel]::new();$panel.Margin=[Windows.Thickness]::new(24);$window.Content=$panel
$editor=[Windows.Controls.TextBox]::new();$editor.Height=40
[Windows.Automation.AutomationProperties]::SetName($editor,'Fixture editor');$panel.Children.Add($editor)|Out-Null
$editor.Add_TextChanged({[IO.File]::WriteAllText((Join-Path $Root 'text.txt'),$editor.Text)})
$other=[Windows.Controls.TextBox]::new();$other.Height=35;[Windows.Automation.AutomationProperties]::SetName($other,'Other editor');$panel.Children.Add($other)|Out-Null
$toggle=[Windows.Controls.CheckBox]::new();$toggle.Content='Fixture toggle';$panel.Children.Add($toggle)|Out-Null
$toggle.Add_Checked({[IO.File]::WriteAllText((Join-Path $Root 'checked.txt'),'true')})
$tree=[Windows.Controls.TreeView]::new();$branch=[Windows.Controls.TreeViewItem]::new();$branch.Header='Fixture branch';$branch.Items.Add('Child node')|Out-Null;$tree.Items.Add($branch)|Out-Null;$panel.Children.Add($tree)|Out-Null
$branch.Add_Expanded({[IO.File]::WriteAllText((Join-Path $Root 'expanded.txt'),'true')})
$button=[Windows.Controls.Button]::new();$button.Content='Fixture action';$button.Height=35;$panel.Children.Add($button)|Out-Null
$button.Add_Click({[IO.File]::WriteAllText((Join-Path $Root 'invoked.txt'),'true')})
$second=[Windows.Window]::new();$second.Title='OpenCowork second fixture';$second.Width=220;$second.Height=150;$second.Left=760;$second.Top=100;$second.Content='Independent target';$second.Show()
$timer=[Windows.Threading.DispatcherTimer]::new();$timer.Interval=[TimeSpan]::FromMilliseconds(100)
$timer.Add_Tick({
 if (Test-Path (Join-Path $Root 'modal')) {Remove-Item -LiteralPath (Join-Path $Root 'modal');$dialog=[Windows.Window]::new();$dialog.Title='OpenCowork modal fixture';$dialog.Owner=$window;$dialog.Width=240;$dialog.Height=140;$close=[Windows.Controls.Button]::new();$close.Content='Close fixture dialog';$close.Add_Click({$dialog.Close()});$dialog.Content=$close;$dialog.ShowDialog()|Out-Null}
 if (Test-Path (Join-Path $Root 'move')) {$window.Left+=20;Remove-Item -LiteralPath (Join-Path $Root 'move')}
 if (Test-Path (Join-Path $Root 'focus-other')) {$other.Focus()|Out-Null;Remove-Item -LiteralPath (Join-Path $Root 'focus-other')}
 if (Test-Path (Join-Path $Root 'cover')) {$window.Topmost=$false;$second.Left=$window.Left;$second.Top=$window.Top;$second.Width=$window.Width;$second.Height=$window.Height;$second.Background=[Windows.Media.Brushes]::Magenta;$second.Topmost=$true;$second.Activate()|Out-Null;Remove-Item -LiteralPath (Join-Path $Root 'cover')}
});$timer.Start()
$window.Add_ContentRendered({$editor.Focus()|Out-Null;[IO.File]::WriteAllText((Join-Path $Root 'ready.txt'),[string]$PID)})
$window.ShowDialog()|Out-Null;$second.Close()
'@ | Set-Content -LiteralPath $fixturePath -Encoding UTF8
$fixture=Start-Process powershell -ArgumentList @('-NoProfile','-STA','-File',('"'+$fixturePath+'"'),'-Root',('"'+$testRoot+'"')) -WindowStyle Hidden -PassThru -RedirectStandardError (Join-Path $testRoot 'error.txt')
try {
    $deadline=(Get-Date).AddSeconds(20)
    while (-not (Test-Path -LiteralPath (Join-Path $testRoot 'ready.txt'))) {if ((Get-Date) -gt $deadline) {throw 'Fixture startup timed out'};Start-Sleep -Milliseconds 100}
    $backend=Join-Path $PSScriptRoot '..\crates\tools\src\computer_window.ps1'
    function Call-Computer([hashtable]$data) {
        $data.statePath=Join-Path $testRoot 'state.json';$data.outputPath=Join-Path $testRoot ('shot-'+[Guid]::NewGuid().ToString('N')+'.jpg')
        $data.captureExecutable=$CaptureExecutable
        if (-not $data.ContainsKey('includeScreenshot')) {$data.includeScreenshot=$false}
        $request=[pscustomobject]$data
        return ((& $backend) | ConvertFrom-Json)
    }
    function Assert-Rejected([hashtable]$data,[string]$code) {
        try {$null=Call-Computer $data} catch {if ($_.Exception.Message -match $code) {return};throw}
        throw "Expected rejection: $code"
    }
    function Input-Computer([string]$action,[hashtable]$extra=@{}) {
        $extra.action=$action;$extra.window=$script:state.window;$extra.stateId=$script:state.stateId
        $script:state=Call-Computer $extra
        if ($script:state.requiresObservation) {throw $script:state.observationError}
    }
    function Element([string]$name) {
        $matches=@($script:state.elements | Where-Object {$_.name -eq $name -and $_.actions.Count -gt 0})
        if ($matches.Count -ne 1) {throw "Expected unique element $name; state: $($script:state | ConvertTo-Json -Depth 8 -Compress)"}
        return $matches[0].index
    }
    $windows=(Call-Computer @{action='list_windows'}).windows
    $selected=@($windows | Where-Object {$_.processId -eq $fixture.Id -and $_.title -eq 'OpenCowork window parity fixture'})
    if ($selected.Count -ne 1 -or @($windows | Where-Object {$_.processId -eq $fixture.Id}).Count -lt 2) {throw 'Window identity selection failed'}
    $target=$selected[0]
    Assert-Rejected @{action='focus';processId=$fixture.Id} 'ambiguous_window'
    $script:state=Call-Computer @{action='activate_window';window=$target}
    if (($script:state.elements.depth | Measure-Object -Maximum).Maximum -le 0) {throw 'Accessibility hierarchy depth is missing'}
    Input-Computer 'click' @{elementIndex=(Element 'Fixture editor')}
    $oldState=$script:state
    $expected=([string][char]0x4e2d)+([char]0x6587)+" Unicode ' `$ literal"
    Input-Computer 'type' @{text=$expected}
    if ([IO.File]::ReadAllText((Join-Path $testRoot 'text.txt')) -ne $expected) {throw 'Unicode input mismatch'}
    Assert-Rejected @{action='type';window=$target;stateId=$oldState.stateId;text='must not repeat'} 'stale_state'
    Input-Computer 'key' @{key='Control_L+a'}
    Input-Computer 'type' @{text='replacement verified'}
    if ([IO.File]::ReadAllText((Join-Path $testRoot 'text.txt')) -ne 'replacement verified') {throw 'Shortcut replacement failed'}
    Input-Computer 'set_value' @{elementIndex=(Element 'Fixture editor');text='ValuePattern verified'}
    if ([IO.File]::ReadAllText((Join-Path $testRoot 'text.txt')) -ne 'ValuePattern verified') {throw 'Direct value update failed'}
    Input-Computer 'secondary_action' @{elementIndex=(Element 'Fixture toggle');secondaryAction='Toggle'}
    if (-not (Test-Path (Join-Path $testRoot 'checked.txt'))) {throw 'TogglePattern failed'}
    Input-Computer 'secondary_action' @{elementIndex=(Element 'Fixture branch');secondaryAction='Expand'}
    if (-not (Test-Path (Join-Path $testRoot 'expanded.txt'))) {throw 'ExpandCollapsePattern failed'}
    Input-Computer 'secondary_action' @{elementIndex=(Element 'Fixture action');secondaryAction='Invoke'}
    if (-not (Test-Path (Join-Path $testRoot 'invoked.txt'))) {throw 'InvokePattern failed'}
    $script:state=Call-Computer @{action='get_window_state';window=$target;includeScreenshot=$true}
    if ($script:state.screenshot.method -ne 'Windows.Graphics.Capture') {throw ('WGC was not used: '+($script:state.screenshot | ConvertTo-Json -Compress))}
    $bitmap=[Drawing.Bitmap]::FromFile($script:state.screenshotPath)
    try {if ($bitmap.Width -ne $script:state.window.width -or $bitmap.Height -ne $script:state.window.height) {throw 'Window screenshot dimensions mismatch'}} finally {$bitmap.Dispose()}
    Assert-Rejected @{action='click';window=$target;stateId=$script:state.stateId;screenshotId='wrong';x=30;y=30} 'stale_screenshot'
    Assert-Rejected @{action='click';window=$target;stateId=$script:state.stateId;elementIndex=999} 'stale_element'
    $previous=$script:state
    [IO.File]::WriteAllText((Join-Path $testRoot 'move'),'move');Start-Sleep -Milliseconds 250
    Assert-Rejected @{action='click';window=$target;stateId=$previous.stateId;x=30;y=30} 'stale_state'
    $script:state=Call-Computer @{action='get_window_state';window=$target}
    Input-Computer 'click' @{elementIndex=(Element 'Fixture editor')}
    [IO.File]::WriteAllText((Join-Path $testRoot 'focus-other'),'focus');Start-Sleep -Milliseconds 250
    Assert-Rejected @{action='type';window=$target;stateId=$script:state.stateId;text='wrong focus'} 'focus_changed'
    Assert-Rejected @{action='click';x=0;y=0} 'target_required'
    $script:state=Call-Computer @{action='get_window_state';window=$target}
    Input-Computer 'key' @{key='Control_L+Shift_L+period'}
    Input-Computer 'key' @{key='F5'}
    Input-Computer 'key' @{key='KP_0'}
    Input-Computer 'scroll' @{x=100;y=200;scrollX=120;scrollY=120}
    [IO.File]::WriteAllText((Join-Path $testRoot 'pointer.txt'),'')
    Input-Computer 'drag' @{fromX=250;fromY=300;x=450;y=350}
    $pointer=[IO.File]::ReadAllText((Join-Path $testRoot 'pointer.txt'))
    if ($pointer -notmatch 'down Left' -or $pointer -notmatch 'up Left') {throw 'Drag did not deliver a complete press/release pair'}
    Input-Computer 'click' @{x=300;y=300;button='middle'}
    $pointer=[IO.File]::ReadAllText((Join-Path $testRoot 'pointer.txt'))
    if ($pointer -notmatch 'down Middle' -or $pointer -notmatch 'up Middle') {throw 'Middle click did not release'}
    $apps=Call-Computer @{action='list_apps'}
    if (-not @($apps.apps | Where-Object {$_.windows.processId -contains $fixture.Id}).Count) {throw 'Running app missing'}
    [IO.File]::WriteAllText((Join-Path $testRoot 'modal'),'modal');Start-Sleep -Milliseconds 350
    $modal=@((Call-Computer @{action='list_windows'}).windows | Where-Object {$_.processId -eq $fixture.Id -and $_.title -eq 'OpenCowork modal fixture'})
    if ($modal.Count -ne 1 -or $modal[0].owner -ne $target.id) {throw 'Modal owner identity missing'}
    $script:state=Call-Computer @{action='activate_window';window=$modal[0]}
    $closed=Call-Computer @{action='secondary_action';window=$script:state.window;stateId=$script:state.stateId;elementIndex=(Element 'Close fixture dialog');secondaryAction='Invoke'}
    if (@((Call-Computer @{action='list_windows'}).windows | Where-Object {$_.title -eq 'OpenCowork modal fixture' -and $_.processId -eq $fixture.Id}).Count) {throw 'Modal did not close'}
    $script:state=Call-Computer @{action='activate_window';window=$target}
    [IO.File]::WriteAllText((Join-Path $testRoot 'cover'),'cover');Start-Sleep -Milliseconds 350
    $coverWindow=@($windows | Where-Object {$_.processId -eq $fixture.Id -and $_.title -eq 'OpenCowork second fixture'})[0]
    if ([CoworkDesktop]::GetForegroundWindow().ToInt64() -ne $coverWindow.id) {throw 'Occlusion fixture did not activate'}
    $covered=Call-Computer @{action='get_window_state';window=$target;includeScreenshot=$true}
    if ($covered.screenshot.method -ne 'Windows.Graphics.Capture') {throw 'Occluded capture did not use WGC'}
    if ([CoworkDesktop]::GetForegroundWindow().ToInt64() -ne $coverWindow.id) {throw 'Capture unexpectedly activated target'}
    $bitmap=[Drawing.Bitmap]::FromFile($covered.screenshotPath)
    try {$pixel=$bitmap.GetPixel(300,400);if ($pixel.G -lt 150) {throw 'Occluded capture contains the magenta covering window'}} finally {$bitmap.Dispose()}
    Write-Output 'PASS: unique windows, Unicode, shortcuts, ValuePattern, Toggle/Expand/Invoke, WGC window screenshot behind an occluder, stale state/index/screenshot and focus protection, function/numpad keys, two-axis scrolling, app catalog, owned modal identity and close'
} finally {if (-not $fixture.HasExited) {Stop-Process -Id $fixture.Id -Force}}
