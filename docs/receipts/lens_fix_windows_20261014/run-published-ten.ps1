$runner = 'C:/Users/mark_/Code/worktrees/turnstone-win-qual/docs/receipts/lens_fix_windows_20261014/run-native-scenario.ps1'
# Lens/accessibility scenes first (the fix's target), then the rest of the ten published cases.
$runs = @(
    @('frame_lens_accessibility','fixed-lens-accessibility','',$false),
    @('frame_stations','fixed-stations','',$false),
    @('frame_stations_reopen','fixed-stations','',$false),
    @('frame_controls_accessibility','fixed-accessibility','',$false),
    @('frame_controls_reopen','fixed-accessibility','',$false),
    @('a4_inline_components','fixed-a4-scoped','',$true),
    @('a4_native_overlap','fixed-alpha','frame_alpha_workshop',$true),
    @('a5_floating_pane','fixed-a5','',$false),
    @('frame_controls_keyboard','fixed-keyboard','',$false),
    @('frame_lens_scroll','fixed-lens-scroll','',$false),
    @('frame_appearance','fixed-fonts','frame_fonts_workshop',$false),
    @('frame_appearance_reopen','fixed-fonts','',$false),
    @('frame_appearance_missing','fixed-missing-fonts','frame_fonts_missing_workshop',$false),
    @('frame_appearance_missing_reopen','fixed-missing-fonts','',$false)
)
foreach ($r in $runs) {
    $args = @('-NoProfile','-File',$runner,'-Scenario',$r[0],'-Case',$r[1])
    if ($r[2]) { $args += @('-Workshop',$r[2]) }
    if ($r[3]) { $args += '-Diagnostics' }
    $out = & pwsh @args 2>&1 | Out-String
    Write-Output ("{0} {1} exit {2}: {3}" -f $r[1], $r[0], $LASTEXITCODE, ($out.Trim() -split "`n" | Select-Object -Last 2) -join ' | ')
}
