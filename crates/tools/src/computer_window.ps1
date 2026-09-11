$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
if (-not ('CoworkDesktop' -as [type])) {
    if (-not $computerNativeSource) {$computerNativeSource=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'computer_native.cs') -Raw}
    Add-Type -TypeDefinition $computerNativeSource
}
if (-not (Get-Command Read-Elements -ErrorAction SilentlyContinue)) {. (Join-Path $PSScriptRoot 'computer_state.ps1')}
[CoworkDesktop]::PhysicalPixels()
# Serialize operations across chat workers. Never automatically replay input.
$mutex=[Threading.Mutex]::new($false,'Local\OpenCowork.Computer.Desktop');$locked=$false
try {
    try {$locked=$mutex.WaitOne(0)} catch [Threading.AbandonedMutexException] {$locked=$true}
    if (-not $locked) {throw 'desktop_busy: Another computer action is running; wait and observe again'}
    $bounds=[Windows.Forms.SystemInformation]::VirtualScreen
    $action=[string]$request.action;$allWindows=@([CoworkDesktop]::Windows());$stateId=[Guid]::NewGuid().ToString('N')
    $result=@{action=$action;ok=$true;desktop=@{x=$bounds.X;y=$bounds.Y;width=$bounds.Width;height=$bounds.Height};coordinateSystem='physical desktop pixels'}
    if ($action -eq 'list_windows') {$result.windows=@($allWindows | ForEach-Object {Window-Object $_});$result | ConvertTo-Json -Depth 12 -Compress;return}
    if ($action -eq 'list_apps') {
        $apps=@{};foreach ($w in $allWindows) {if (-not $apps.ContainsKey($w.App)) {$apps[$w.App]=@{id=$w.App;displayName=[IO.Path]::GetFileNameWithoutExtension($w.App);isRunning=$true;windows=@()}};$apps[$w.App].windows+=@(Window-Object $w)}
        if (Get-Command Get-StartApps -ErrorAction SilentlyContinue) {foreach ($app in @(Get-StartApps | Select-Object -First 500)) {if (-not $apps.ContainsKey($app.AppID)) {$apps[$app.AppID]=@{id=$app.AppID;displayName=$app.Name;isRunning=$false;windows=@()}}}}
        $result.apps=@($apps.Values);$result | ConvertTo-Json -Depth 12 -Compress;return
    }
    if ($action -eq 'launch_app') {
        $app=[string]$request.app
        $info=[Diagnostics.ProcessStartInfo]::new();$info.UseShellExecute=$true
        if ([IO.Path]::IsPathRooted($app) -and [IO.Path]::GetExtension($app) -eq '.exe' -and (Test-Path -LiteralPath $app -PathType Leaf)) {$info.FileName=$app}
        else {
            $installed=@(Get-StartApps | Where-Object {$_.AppID -ceq $app})
            if ($installed.Count -ne 1 -or $app -match '["\r\n]') {throw 'unknown_app: Use an exact installed app ID or absolute .exe path'}
            $info.FileName='explorer.exe';$info.Arguments='shell:AppsFolder\'+$app
        }
        [void][Diagnostics.Process]::Start($info)
        $result.next='List windows and choose the unique intended window after launch';$result | ConvertTo-Json -Compress;return
    }
    $window=$null
    if ($request.window) {
        if (-not $request.window.id -or -not $request.window.processId -or -not $request.window.processStarted) {throw 'invalid_window: Use the window object returned by list_windows'}
        $window=Window-Object ([CoworkDesktop]::Describe([IntPtr][long]$request.window.id))
        if ($window.processId -ne $request.window.processId -or $window.processStarted -ne [string]$request.window.processStarted) {throw 'stale_window: Window identity changed; list windows again'}
    } elseif ($action -eq 'focus' -and $request.processId) {
        $matches=@($allWindows | Where-Object {$_.Id -eq $request.processId})
        if ($matches.Count -ne 1) {throw 'ambiguous_window: Select a unique window from list_windows'}
        $window=Window-Object $matches[0]
    }
    if ($action -in @('get_window','activate_window','get_window_state','set_value','secondary_action','focus') -and -not $window) {throw 'target_required: Select a window using list_windows first'}
    if ($action -eq 'get_window') {$result.window=$window;$result | ConvertTo-Json -Compress;return}
    $mutating=$action -in @('click','double_click','drag','move','type','key','scroll','set_value','secondary_action')
    $observed=$null;$element=$null
    if ($mutating) {
        if (-not $window) {throw 'target_required: Input requires a window and stateId from get_window_state'}
        if (-not $request.statePath -or -not (Test-Path -LiteralPath $request.statePath)) {throw 'stale_state: Observe the target window before input'}
        $observed=Get-Content -LiteralPath $request.statePath -Raw -Encoding UTF8 | ConvertFrom-Json
        if (-not $request.stateId -or $request.stateId -ne $observed.stateId -or $observed.window.id -ne $window.id -or $observed.window.processStarted -ne $window.processStarted -or ([DateTime]::UtcNow-[DateTime]::Parse($observed.capturedAt)).TotalSeconds -gt 120) {throw 'stale_state: Observation expired or belongs to another window; observe again'}
        foreach ($field in @('x','y','width','height')) {if ($window[$field] -ne $observed.window.$field) {throw 'stale_state: Window moved or resized; observe again'}}
        if ($request.screenshotId -and $request.screenshotId -ne $observed.screenshot.id) {throw 'stale_screenshot: Refresh window state'}
        if ($null -ne $request.elementIndex) {
            $cached=@($observed.elements | Where-Object {$_.index -eq $request.elementIndex})
            if ($cached.Count -ne 1) {throw 'stale_element: Index was not in the observation'}
            $tree=Read-Elements ([IntPtr]$window.id) $window.x $window.y
            $found=@($tree.rows | Where-Object {$_.runtimeId -eq $cached[0].runtimeId -and $_.name -ceq $cached[0].name -and $_.role -eq $cached[0].role})
            if ($found.Count -ne 1) {throw 'stale_element: Control changed; observe again'}
            foreach ($field in @('left','top','width','height')) {if ($found[0].$field -ne $cached[0].$field) {throw 'stale_element: Control moved; observe again'}}
            $element=$tree.objects[$found[0].index]
            if (-not $element.Current.IsEnabled -or $element.Current.IsPassword) {throw 'element_unavailable: Control is disabled or protected'}
        }
        if ($action -in @('set_value','secondary_action') -and -not $element) {throw 'element_required: Use an observed elementIndex'}
        if (-not [CoworkDesktop]::Focus([IntPtr]$window.id)) {throw 'foreground_not_target: Windows refused activation; observe again'}
        $activated=Window-Object ([CoworkDesktop]::Describe([IntPtr]$window.id))
        foreach ($field in @('x','y','width','height')) {if ($activated[$field] -ne $window[$field]) {throw 'stale_state: Activation changed window geometry; observe again'}}
        if ($action -in @('type','key')) {
            $focus=Focus-Identity
            if ([CoworkDesktop]::GetForegroundWindow() -eq [IntPtr]::Zero -or $focus.password -or -not $observed.focusKey -or $observed.focusKey -ne $focus.key) {throw 'focus_changed: Observe and confirm the focused control before typing or pressing keys'}
        }
        # Consume before input: a partial/failed action must not be replayed.
        [IO.File]::WriteAllText($request.statePath,'{}')
    }
    if ($action -in @('activate_window','focus')) {if (-not [CoworkDesktop]::Focus([IntPtr]$window.id)) {throw 'foreground_not_target: Windows refused activation'}}
    if ($action -eq 'wait') {$ms=500;if ($null -ne $request.waitMs) {$ms=[int]$request.waitMs};if ($ms -lt 0 -or $ms -gt 5000) {throw 'waitMs must be 0..5000'};Start-Sleep -Milliseconds $ms}
    if ($action -in @('click','double_click','move','drag','scroll')) {
        $x=$request.x;$y=$request.y
        if ($element) {$rect=$element.Current.BoundingRectangle;$x=[int]($rect.X+$rect.Width/2)-$window.x;$y=[int]($rect.Y+$rect.Height/2)-$window.y}
        $point=Assert-Point $window $x $y
        if ($action -eq 'drag') {
            $start=Assert-Point $window $request.fromX $request.fromY
            [CoworkDesktop]::Drag($start[0],$start[1],$point[0],$point[1])
        } else {[void][CoworkDesktop]::SetCursorPos($point[0],$point[1])}
        if ($action -in @('click','double_click')) {
            $down=2;$up=4;if ($request.button -eq 'right') {$down=8;$up=16};if ($request.button -eq 'middle') {$down=32;$up=64}
            $count=1;if ($action -eq 'double_click') {$count=2};if ($null -ne $request.clickCount) {$count=[int]$request.clickCount};if ($count -lt 1 -or $count -gt 3) {throw 'clickCount must be 1..3'}
            [CoworkDesktop]::Click($down,$up,$count)
        }
        if ($action -eq 'scroll') {
            $sx=[int]$request.scrollX;$sy=[int]$request.scrollY;if ($null -ne $request.amount) {$sy=-[int]$request.amount}
            if ([Math]::Abs($sx) -gt 2400 -or [Math]::Abs($sy) -gt 2400) {throw 'Scroll deltas must be -2400..2400'}
            if ($sy) {[CoworkDesktop]::Mouse(2048,-$sy)};if ($sx) {[CoworkDesktop]::Mouse(4096,$sx)}
        }
    }
    if ($action -eq 'type') {[CoworkDesktop]::TypeText([string]$request.text)}
    if ($action -eq 'key') {[CoworkDesktop]::Key([string]$request.key)}
    if ($action -eq 'set_value') {
        $value=Pattern $element ([System.Windows.Automation.ValuePattern]::Pattern)
        if (-not $value -or $value.Current.IsReadOnly) {throw 'unsupported_pattern: No writable ValuePattern; click, observe focus, and type instead'}
        $value.SetValue([string]$request.text)
    }
    if ($action -eq 'secondary_action') {
        $p=$null
        switch ([string]$request.secondaryAction) {
            'Invoke' {$p=Pattern $element ([System.Windows.Automation.InvokePattern]::Pattern);if ($p) {$p.Invoke()}}
            'Toggle' {$p=Pattern $element ([System.Windows.Automation.TogglePattern]::Pattern);if ($p) {$p.Toggle()}}
            'Expand' {$p=Pattern $element ([System.Windows.Automation.ExpandCollapsePattern]::Pattern);if ($p) {$p.Expand()}}
            'Collapse' {$p=Pattern $element ([System.Windows.Automation.ExpandCollapsePattern]::Pattern);if ($p) {$p.Collapse()}}
            'Select' {$p=Pattern $element ([System.Windows.Automation.SelectionItemPattern]::Pattern);if ($p) {$p.Select()}}
            'ScrollIntoView' {$p=Pattern $element ([System.Windows.Automation.ScrollItemPattern]::Pattern);if ($p) {$p.ScrollIntoView()}}
            {$_ -in @('Scroll Up','Scroll Down','Scroll Left','Scroll Right')} {
                $p=Pattern $element ([System.Windows.Automation.ScrollPattern]::Pattern)
                if ($p) {
                    $horizontal=[System.Windows.Automation.ScrollAmount]::NoAmount;$vertical=$horizontal
                    switch ($request.secondaryAction) {
                        'Scroll Up' {$vertical=[System.Windows.Automation.ScrollAmount]::SmallDecrement}
                        'Scroll Down' {$vertical=[System.Windows.Automation.ScrollAmount]::SmallIncrement}
                        'Scroll Left' {$horizontal=[System.Windows.Automation.ScrollAmount]::SmallDecrement}
                        'Scroll Right' {$horizontal=[System.Windows.Automation.ScrollAmount]::SmallIncrement}
                    }
                    $p.Scroll($horizontal,$vertical)
                }
            }
            default {throw 'unsupported_secondary_action: Use an action listed on the element'}
        }
        if (-not $p) {throw 'unsupported_pattern: Observe supported actions again'}
    }
    if ($window -or $action -in @('snapshot','screenshot')) {
        # A failed observation also invalidates the previously cached state.
        if ($request.statePath) {[IO.File]::WriteAllText($request.statePath,'{}')}
        try {
            if ($mutating) {Start-Sleep -Milliseconds 80}
            if ($window) {
                $window=Window-Object ([CoworkDesktop]::Describe([IntPtr]$window.id));$result.window=$window;$result.coordinateSystem='physical pixels relative to window top-left';$originX=$window.x;$originY=$window.y;$handle=[IntPtr]$window.id
                $result.relatedWindows=@([CoworkDesktop]::Windows() | Where-Object {$_.Owner -eq $window.id -or ($window.owner -ne 0 -and $_.Handle -eq $window.owner)} | ForEach-Object {Window-Object $_})
            }
            else {$result.windows=$allWindows;$originX=0;$originY=0;$handle=[CoworkDesktop]::GetForegroundWindow()}
            $result.stateId=$stateId;$result.capturedAt=[DateTime]::UtcNow.ToString('o')
            if ($window -and [CoworkDesktop]::GetForegroundWindow().ToInt64() -eq $window.id) {$result.focusKey=(Focus-Identity).key}
            if ($request.includeText -ne $false -and $action -ne 'screenshot' -and $handle -ne [IntPtr]::Zero) {
                $tree=Read-Elements $handle $originX $originY;$result.elements=$tree.rows;$result.truncated=$tree.truncated
                $focused=@($tree.rows | Where-Object {$_.focused}) | Select-Object -First 1
                if ($focused) {$result.focusedRuntimeId=$focused.runtimeId;$result.focusedElement=$focused}
                $result.selectedElements=@($tree.rows | Where-Object {$_.selected})
                $result.documentText=Short-Text ((@($tree.rows | Where-Object {$_.documentText} | Select-Object -First 2 | ForEach-Object {$_.documentText})) -join "`n")
                $result.selectedText=Short-Text ((@($tree.rows | Where-Object {$_.selectedText} | ForEach-Object {$_.selectedText})) -join "`n")
            }
            if ($action -eq 'screenshot' -or ($window -and $request.includeScreenshot -ne $false)) {$result.screenshot=Capture $window ([string]$request.outputPath);$result.screenshotPath=$request.outputPath}
            if ($window -and $request.statePath) {[IO.File]::WriteAllText($request.statePath,($result | ConvertTo-Json -Depth 12 -Compress),[Text.UTF8Encoding]::new($false))}
        } catch {
            if (-not $mutating) {throw}
            $result.observationError=$_.Exception.Message;$result.requiresObservation=$true;$result.Remove('stateId');$result.next='Action executed; get fresh window state. Do not repeat the action blindly.'
        }
    }
    $result | ConvertTo-Json -Depth 12 -Compress
} finally {if ($locked) {$mutex.ReleaseMutex()};$mutex.Dispose()}
