// Narrow correction for the pinned Spout2 SDK. Its GetRefreshRate enumerates
// every supported mode and returns the last mode, rather than the active one.
// Checking the full old function makes SDK upgrades fail visibly for review.
pub fn current_refresh_rate(source: &str) -> String {
    let source = source.replace("\r\n", "\n");
    let old = "\tdouble GetRefreshRate()\n\t{\n\t\tdouble frequency = 60.0; // default\n\t\tDEVMODE DevMode = {};\n\t\tBOOL bResult = true;\n\t\tDWORD dwCurrentSettings = 0;\n\t\tDevMode.dmSize = sizeof(DEVMODE);\n\t\t// Test all the graphics modes\n\t\t// https://docs.microsoft.com/en-us/windows/desktop/api/winuser/nf-winuser-enumdisplaysettingsa\n\t\twhile (bResult) {\n\t\t\tbResult = EnumDisplaySettings(NULL, dwCurrentSettings, &DevMode);\n\t\t\tif (bResult)\n\t\t\t\tfrequency = static_cast<double>(DevMode.dmDisplayFrequency);\n\t\t\tdwCurrentSettings++;\n\t\t}\n\t\treturn frequency;\n\t}";
    let new = "\tdouble GetRefreshRate()\n\t{\n\t\tDEVMODE mode = {};\n\t\tmode.dmSize = sizeof(mode);\n\t\tif (EnumDisplaySettings(NULL, ENUM_CURRENT_SETTINGS, &mode) && mode.dmDisplayFrequency > 1)\n\t\t\treturn static_cast<double>(mode.dmDisplayFrequency);\n\t\treturn 60.0; // Unavailable or unspecified rate\n\t}";
    assert_eq!(
        source.matches(old).count(),
        1,
        "pinned Spout2 GetRefreshRate changed; review the compatibility fix"
    );
    source.replacen(old, new, 1)
}
