function Window-Object($info) {
    return @{id=$info.Handle;processId=$info.Id;app=$info.App;title=$info.MainWindowTitle;processStarted=[string]$info.ProcessStarted;owner=$info.Owner;minimized=$info.Minimized;x=$info.X;y=$info.Y;width=$info.Width;height=$info.Height}
}
function Short-Text($text, $limit=6000) {
    if ($null -eq $text) { return '' };$text=[string]$text
    return $text.Substring(0,[Math]::Min($limit,$text.Length))
}
function Pattern($element, $pattern) {
    $value=$null
    if ($element.TryGetCurrentPattern($pattern,[ref]$value)) { return $value }
    return $null
}
function Focus-Identity {
    $focused=$null
    try {$focused=[System.Windows.Automation.AutomationElement]::FocusedElement} catch {}
    if ($focused) {return @{key=($focused.GetRuntimeId() -join '.');password=$focused.Current.IsPassword}}
    return @{key=('hwnd:'+[CoworkDesktop]::FocusHandle());password=$false}
}
function Read-Elements($handle,$originX,$originY) {
    $rows=[Collections.Generic.List[object]]::new();$objects=[Collections.Generic.List[object]]::new()
    $root=[System.Windows.Automation.AutomationElement]::FromHandle($handle)
    $queue=[Collections.Generic.Queue[object]]::new();$queue.Enqueue(@($root,0))
    $walker=[System.Windows.Automation.TreeWalker]::ControlViewWalker
    $watch=[Diagnostics.Stopwatch]::StartNew()
    while ($queue.Count -gt 0 -and $rows.Count -lt 200 -and $watch.ElapsedMilliseconds -lt 2500) {
        $entry=$queue.Dequeue();$element=$entry[0];$depth=[int]$entry[1]
        try {
            $info=$element.Current;$rect=$info.BoundingRectangle
            if (-not $info.IsOffscreen -and -not $rect.IsEmpty) {
                $actions=[Collections.Generic.List[string]]::new()
                $row=@{index=$rows.Count;name=(Short-Text $info.Name 512);role=$info.ControlType.ProgrammaticName;className=$info.ClassName;automationId=$info.AutomationId;focused=$info.HasKeyboardFocus;enabled=$info.IsEnabled;password=$info.IsPassword;depth=$depth;runtimeId=($element.GetRuntimeId() -join '.');x=[int]($rect.X+$rect.Width/2)-$originX;y=[int]($rect.Y+$rect.Height/2)-$originY;left=[int]$rect.X-$originX;top=[int]$rect.Y-$originY;width=[int]$rect.Width;height=[int]$rect.Height}
                if (-not $info.IsPassword) {
                    $value=Pattern $element ([System.Windows.Automation.ValuePattern]::Pattern)
                    if ($value) { $row.value=Short-Text $value.Current.Value;if (-not $value.Current.IsReadOnly) {$actions.Add('SetValue')} }
                    $text=Pattern $element ([System.Windows.Automation.TextPattern]::Pattern)
                    if ($text) {
                        $row.documentText=Short-Text ($text.DocumentRange.GetText(6000))
                        $row.selectedText=Short-Text ((@($text.GetSelection()) | ForEach-Object {$_.GetText(2000)}) -join "`n") 2000
                    }
                }
                if (Pattern $element ([System.Windows.Automation.InvokePattern]::Pattern)) {$actions.Add('Invoke')}
                if (Pattern $element ([System.Windows.Automation.TogglePattern]::Pattern)) {$actions.Add('Toggle')}
                if (Pattern $element ([System.Windows.Automation.ExpandCollapsePattern]::Pattern)) {$actions.Add('Expand');$actions.Add('Collapse')}
                if (Pattern $element ([System.Windows.Automation.SelectionItemPattern]::Pattern)) {
                    $actions.Add('Select');$row.selected=(Pattern $element ([System.Windows.Automation.SelectionItemPattern]::Pattern)).Current.IsSelected
                }
                if (Pattern $element ([System.Windows.Automation.ScrollItemPattern]::Pattern)) {$actions.Add('ScrollIntoView')}
                $scroll=Pattern $element ([System.Windows.Automation.ScrollPattern]::Pattern)
                if ($scroll) {
                    if ($scroll.Current.VerticallyScrollable) {$actions.Add('Scroll Up');$actions.Add('Scroll Down')}
                    if ($scroll.Current.HorizontallyScrollable) {$actions.Add('Scroll Left');$actions.Add('Scroll Right')}
                }
                $row.actions=$actions.ToArray();$rows.Add($row);$objects.Add($element)
            }
            if ($depth -lt 8) {
                $child=$walker.GetFirstChild($element);$count=0
                while ($null -ne $child -and $count -lt 60 -and $queue.Count -lt 300) {$queue.Enqueue(@($child,($depth+1)));$child=$walker.GetNextSibling($child);$count++}
            }
        } catch { }
    }
    return @{rows=$rows.ToArray();objects=$objects.ToArray();truncated=($queue.Count -gt 0)}
}
function Assert-Point($window,$px,$py) {
    if ($null -eq $px -or $null -eq $py) {throw 'invalid_input: x and y are required'}
    if ($px -lt 0 -or $py -lt 0 -or $px -ge $window.width -or $py -ge $window.height) {throw 'invalid_coordinates: Point is outside target window'}
    $px+=$window.x;$py+=$window.y
    if (-not [CoworkDesktop]::OwnsPoint([IntPtr]$window.id,$px,$py)) {throw 'point_occluded: Point belongs to another window or modal; list windows and observe again'}
    if (-not $bounds.Contains([int]$px,[int]$py)) {throw 'invalid_coordinates: Point is outside desktop'}
    return @([int]$px,[int]$py)
}
function Capture($window,$path) {
    if ($window) {$width=$window.width;$height=$window.height} else {$width=$bounds.Width;$height=$bounds.Height}
    if ($width -le 0 -or $height -le 0 -or [long]$width*$height -gt 40000000) {throw 'capture_bounds_invalid: Capture a smaller region or restore the window'}
    $bitmap=[Drawing.Bitmap]::new($width,$height);$graphics=[Drawing.Graphics]::FromImage($bitmap)
    try {
        $method=$null;$captureWarning=$null
        if ($window -and $request.captureMode -ne 'visible' -and $request.captureExecutable) {
            $rawPath=$path+'.wgc.jpg';$captureProcess=$null
            try {
                if ($window.minimized) {throw 'Window is minimized'}
                $info=[Diagnostics.ProcessStartInfo]::new();$info.FileName=$request.captureExecutable
                $info.Arguments='--computer-capture '+$window.id+' "'+$rawPath+'"'
                $info.UseShellExecute=$false;$info.CreateNoWindow=$true;$info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
                $captureProcess=[Diagnostics.Process]::Start($info)
                if (-not $captureProcess.WaitForExit(7000)) {$captureProcess.Kill();$captureProcess.WaitForExit();throw 'Graphics Capture timed out'}
                if ($captureProcess.ExitCode -ne 0) {throw (Short-Text ($captureProcess.StandardError.ReadToEnd()) 500)}
                $captureMeta=$captureProcess.StandardOutput.ReadToEnd() | ConvertFrom-Json
                $captureRect=[CoworkDesktop]::CaptureRect([IntPtr]$window.id)
                $image=[Drawing.Bitmap]::FromFile($rawPath)
                try {
                    if ($image.Width -eq $width -and $image.Height -eq $height) {$dx=0;$dy=0}
                    elseif ($image.Width -eq $captureRect.Right-$captureRect.Left -and $image.Height -eq $captureRect.Bottom-$captureRect.Top) {$dx=$captureRect.Left-$window.x;$dy=$captureRect.Top-$window.y}
                    else {throw 'Capture dimensions changed; observe again'}
                    $graphics.DrawImageUnscaled($image,[int]$dx,[int]$dy)
                } finally {$image.Dispose()}
                $method='Windows.Graphics.Capture';$originX=$window.x;$originY=$window.y
            } catch {$captureWarning=$_.Exception.Message}
            finally {if ($captureProcess) {$captureProcess.Dispose()};if (Test-Path -LiteralPath $rawPath) {Remove-Item -LiteralPath $rawPath}}
        }
        if (-not $method -and $window -and $request.captureMode -ne 'visible') {
            if ($window.minimized) {throw 'window_minimized: Restore and observe the window before capture'}
            $dc=$graphics.GetHdc()
            try {$ok=[CoworkDesktop]::PrintWindow([IntPtr]$window.id,$dc,2)} finally {$graphics.ReleaseHdc($dc)}
            if (-not $ok) {throw 'capture_failed: App does not support PrintWindow. Activate and retry with captureMode=visible'}
            $method='PrintWindow';$originX=$window.x;$originY=$window.y
        } elseif (-not $method) {
            if ($window) {
                if ([CoworkDesktop]::GetForegroundWindow().ToInt64() -ne $window.id) {throw 'foreground_not_target: Activate window before visible capture'}
                $originX=$window.x;$originY=$window.y
            } else {$originX=$bounds.X;$originY=$bounds.Y}
            $graphics.CopyFromScreen($originX,$originY,0,0,[Drawing.Size]::new($width,$height));$method='CopyFromScreen'
        }
        $bitmap.Save($path,[Drawing.Imaging.ImageFormat]::Jpeg)
        if ((Get-Item -LiteralPath $path).Length -gt 8MB) {throw 'screenshot_too_large: Capture a smaller window (8 MB limit)'}
        return @{id=$stateId;width=$width;height=$height;originX=$originX;originY=$originY;method=$method;fallbackReason=$captureWarning}
    } finally {$graphics.Dispose();$bitmap.Dispose()}
}
