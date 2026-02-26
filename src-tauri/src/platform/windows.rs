mod smart;

use crate::commands::disk::DiskDiagnostics;
use crate::commands::disk::DiskInfo;
use crate::commands::disk::SmartAttribute;
use crate::utils::command::CommandExecutor;
use crate::{AppError, Result};
use regex::Regex;
use serde_json::{Map, Value};
use std::collections::HashSet;
use tracing::{info, warn};

/// PowerShell script to get disk info with volume letters.
/// For each disk, we query its partitions → volumes → drive letters.
const PS_LIST_DISKS: &str = r#"
$physical = Get-PhysicalDisk | Select-Object FriendlyName, MediaType, SpindleSpeed
$disks = Get-Disk | Select-Object Number, FriendlyName, Size, BusType, MediaType, IsBoot, IsSystem
$systemDiskNumber = $null
try {
    $os = Get-CimInstance Win32_OperatingSystem -ErrorAction Stop
    $systemDrive = if ($os) { [string]$os.SystemDrive } else { '' }
    if ($systemDrive) {
        $driveLetter = $systemDrive.Trim().TrimEnd('\').TrimEnd(':')
        if ($driveLetter) {
            $sysPart = Get-Partition -DriveLetter $driveLetter -ErrorAction SilentlyContinue | Select-Object -First 1
            if ($sysPart -and $null -ne $sysPart.DiskNumber) {
                $systemDiskNumber = [int]$sysPart.DiskNumber
            }
        }
    }
} catch {}
$result = @()
foreach ($d in $disks) {
    # Try to resolve media type (SSD/HDD)
    $pd = $physical | Where-Object { $_.FriendlyName -eq $d.FriendlyName } | Select-Object -First 1
    $media = $d.MediaType
    if (-not $media -or $media -eq 'Unspecified' -or $media -eq '') {
        if ($pd) { $media = $pd.MediaType }
    }
    $busRaw = [string]$d.BusType
    if (-not $media -or $media -eq 'Unspecified' -or $media -eq '') {
        if ($busRaw -match '(^17$|NVMe)') {
            $media = 'SSD'
        } elseif ($pd -and $pd.SpindleSpeed -eq 0) {
            $media = 'SSD'
        } elseif ($pd -and $pd.SpindleSpeed -gt 0) {
            $media = 'HDD'
        } elseif ($d.FriendlyName -match '(?i)SSD|NVME') {
            $media = 'SSD'
        } else {
            $media = 'Unknown'
        }
    }

    $volumes = Get-Partition -DiskNumber $d.Number -ErrorAction SilentlyContinue |
        Get-Volume -ErrorAction SilentlyContinue |
        Where-Object { $_.DriveLetter -ne $null -and $_.DriveLetter -ne '' } |
        Select-Object -ExpandProperty DriveLetter
    $vol = if ($volumes -is [array]) { $volumes[0] } else { $volumes }
    $derivedSystem = if ($null -ne $systemDiskNumber) { [int]$d.Number -eq $systemDiskNumber } else { [bool]($d.IsBoot -or $d.IsSystem) }
    $busName = switch -Regex ($busRaw) {
        '^0$|^Unknown$' { "Unknown" }
        '^1$|^SCSI$' { "SCSI" }
        '^2$|^ATAPI$' { "ATAPI" }
        '^3$|^ATA$' { "ATA" }
        '^4$|^1394$' { "1394" }
        '^5$|^SSA$' { "SSA" }
        '^6$|^FibreChannel$' { "FibreChannel" }
        '^7$|^USB$' { "USB" }
        '^8$|^RAID$' { "RAID" }
        '^9$|^iSCSI$' { "iSCSI" }
        '^10$|^SAS$' { "SAS" }
        '^11$|^SATA$' { "SATA" }
        '^12$|^SD$' { "SD" }
        '^13$|^MMC$' { "MMC" }
        '^15$|^Virtual$' { "Virtual" }
        '^16$|^StorageSpaces$' { "StorageSpaces" }
        '^17$|^NVMe$' { "NVMe" }
        default { if ($busRaw) { $busRaw } else { "Other" } }
    }
    $result += [PSCustomObject]@{
        Number       = $d.Number
        FriendlyName = $d.FriendlyName
        Size         = $d.Size
        BusType      = $d.BusType
        MediaType    = $media
        IsBoot       = $d.IsBoot
        IsSystem     = [bool]$derivedSystem
        BusTypeName  = $busName
        VolumeLetter = if ($vol) { [string]$vol } else { "" }
    }
}
$result | ConvertTo-Json
"#;

/// PowerShell script to get detailed disk diagnostics including SMART/reliability.
const PS_LIST_DISK_DIAGNOSTICS: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'

function Normalize-Text([object]$value) {
    if ($null -eq $value) { return '' }
    return ([string]$value).Trim()
}

function Get-BusTypeName([object]$bus) {
    $raw = Normalize-Text $bus
    switch -Regex ($raw) {
        '^0$|^Unknown$' { 'Unknown' }
        '^1$|^SCSI$' { 'SCSI' }
        '^2$|^ATAPI$' { 'ATAPI' }
        '^3$|^ATA$' { 'ATA' }
        '^4$|^1394$' { '1394' }
        '^5$|^SSA$' { 'SSA' }
        '^6$|^FibreChannel$' { 'FibreChannel' }
        '^7$|^USB$' { 'USB' }
        '^8$|^RAID$' { 'RAID' }
        '^9$|^iSCSI$' { 'iSCSI' }
        '^10$|^SAS$' { 'SAS' }
        '^11$|^SATA$' { 'SATA' }
        '^12$|^SD$' { 'SD' }
        '^13$|^MMC$' { 'MMC' }
        '^15$|^Virtual$' { 'Virtual' }
        '^16$|^StorageSpaces$' { 'StorageSpaces' }
        '^17$|^NVMe$' { 'NVMe' }
        default { if ($raw) { $raw } else { 'Unknown' } }
    }
}

function Is-UsbDevice([string]$busName, [string]$interfaceType, [string]$pnp) {
    return ($busName -eq 'USB') -or ($interfaceType -eq 'USB') -or ($pnp -like 'USB*') -or ($pnp -like '*USBSTOR*')
}

function Resolve-MediaType([string]$rawMedia, [object]$physicalDisk, [string]$busName, [string]$model, [string]$interfaceType) {
    $media = Normalize-Text $rawMedia
    if (-not $media -or $media -eq 'Unspecified') {
        if ($physicalDisk -and $physicalDisk.MediaType) {
            $media = Normalize-Text $physicalDisk.MediaType
        }
    }

    if ($media -and $media -ne 'Unspecified') {
        if ($media -match '(?i)SSD') { return 'SSD' }
        if ($media -match '(?i)HDD|Rotational') { return 'HDD' }
        return $media
    }

    if ($busName -eq 'NVMe' -or $model -match '(?i)NVME|SSD' -or $interfaceType -eq 'NVMExpress') {
        return 'SSD'
    }

    if ($physicalDisk -and $null -ne $physicalDisk.SpindleSpeed) {
        if ([int64]$physicalDisk.SpindleSpeed -eq 0) { return 'SSD' }
        if ([int64]$physicalDisk.SpindleSpeed -gt 0) { return 'HDD' }
    }

    if ($model -match '(?i)\bHDD\b|ST\d{3,}|WDC|WD\d{3,}|TOSHIBA|HITACHI') {
        return 'HDD'
    }

    return 'Unknown'
}

function Test-MaskedSerial([string]$serial) {
    $v = Normalize-Text $serial
    if (-not $v) { return $true }

    $normalized = ($v -replace '[^0-9A-Za-z]', '').ToUpperInvariant()
    if (-not $normalized) { return $true }
    if ($normalized.Length -lt 4) { return $true }
    if ($normalized -match '^[0]+$') { return $true }
    if ($normalized -match '^[F]+$') { return $true }
    if ($normalized -match '^[D0]+$') { return $true }
    if ($normalized -match '^0{8,}D0{8,}$') { return $true }
    return $false
}

function Get-FirstValidSerial([object[]]$candidates) {
    foreach ($candidate in $candidates) {
        $serial = Normalize-Text $candidate
        if (-not $serial) { continue }
        if (-not (Test-MaskedSerial $serial)) {
            return $serial
        }
    }
    return ''
}

function Get-SmartName([int]$id) {
    switch ($id) {
        1 { 'Read Error Rate' }
        2 { 'Throughput Performance' }
        3 { 'Spin-Up Time' }
        4 { 'Start/Stop Count' }
        5 { 'Reallocated Sectors Count' }
        7 { 'Seek Error Rate' }
        8 { 'Seek Time Performance' }
        9 { 'Power-On Hours' }
        10 { 'Spin Retry Count' }
        11 { 'Calibration Retry Count' }
        12 { 'Power Cycle Count' }
        170 { 'Available Reserved Space' }
        171 { 'Program Fail Count' }
        172 { 'Erase Fail Count' }
        173 { 'Wear Leveling Count' }
        174 { 'Unexpected Power Loss Count' }
        177 { 'Wear Range Delta' }
        179 { 'Used Reserved Block Count Total' }
        180 { 'Unused Reserved Block Count Total' }
        181 { 'Program Fail Count Total' }
        182 { 'Erase Fail Count' }
        183 { 'Runtime Bad Block' }
        184 { 'End-to-End Error' }
        187 { 'Reported Uncorrectable Errors' }
        188 { 'Command Timeout' }
        190 { 'Airflow Temperature' }
        191 { 'G-Sense Error Rate' }
        192 { 'Power-Off Retract Count' }
        193 { 'Load/Unload Cycle Count' }
        194 { 'Temperature' }
        195 { 'Hardware ECC Recovered' }
        196 { 'Reallocation Event Count' }
        197 { 'Current Pending Sector Count' }
        198 { 'Offline Uncorrectable Sector Count' }
        199 { 'UltraDMA CRC Error Count' }
        200 { 'Write Error Rate' }
        201 { 'Soft Read Error Rate' }
        202 { 'Data Address Mark Errors' }
        206 { 'Flying Height' }
        210 { 'Vibration During Write' }
        211 { 'Vibration During Write Time' }
        212 { 'Shock During Write' }
        225 { 'Load/Unload Retry Count' }
        226 { 'Load-in Time' }
        227 { 'Torque Amplification Count' }
        228 { 'Power-Off Retract Cycle' }
        230 { 'Drive Life Protection Status' }
        231 { 'SSD Life Left' }
        232 { 'Available Reserved Space' }
        233 { 'Media Wearout Indicator' }
        234 { 'Average Erase Count' }
        241 { 'Total LBAs Written' }
        242 { 'Total LBAs Read' }
        246 { 'Total NAND Writes' }
        247 { 'Host Program NAND Pages Count' }
        248 { 'FTL Program NAND Pages Count' }
        249 { 'NAND Writes 1GiB' }
        250 { 'Read Error Retry Rate' }
        default { "Attribute $id" }
    }
}

function Parse-SmartAttributes([byte[]]$vendor, [byte[]]$thresholds) {
    $attrs = @()
    if (-not $vendor -or $vendor.Length -lt 362) { return @() }
    for ($i = 2; $i -lt 362; $i += 12) {
        $id = [int]$vendor[$i]
        if ($id -eq 0) { continue }

        $current = [int]$vendor[$i + 3]
        $worst = [int]$vendor[$i + 4]
        [uint64]$raw = 0
        for ($j = 0; $j -lt 6; $j++) {
            $raw += ([uint64]$vendor[$i + 5 + $j] -shl (8 * $j))
        }

        $threshold = $null
        if ($thresholds -and $thresholds.Length -ge 362) {
            for ($k = 2; $k -lt 362; $k += 12) {
                if ([int]$thresholds[$k] -eq $id) {
                    $threshold = [int]$thresholds[$k + 1]
                    break
                }
            }
        }

        $attrs += [PSCustomObject]@{
            id = $id
            name = Get-SmartName $id
            current = $current
            worst = $worst
            threshold = $threshold
            raw = $raw
            raw_hex = ('0x{0:X12}' -f $raw)
        }
    }
    return $attrs
}

function Get-SmartAttr([object[]]$attrs, [int]$id) {
    if (-not $attrs) { return $null }
    return $attrs | Where-Object { [int]$_.id -eq $id } | Select-Object -First 1
}

function Get-SmartRaw([object[]]$attrs, [int]$id) {
    $attr = Get-SmartAttr $attrs $id
    if ($attr -and $null -ne $attr.raw) {
        return [uint64]$attr.raw
    }
    return $null
}

function Get-SmartCurrent([object[]]$attrs, [int]$id) {
    $attr = Get-SmartAttr $attrs $id
    if ($attr -and $null -ne $attr.current) {
        return [int]$attr.current
    }
    return $null
}

function Get-FirstSmartRaw([object[]]$attrs, [int[]]$ids) {
    foreach ($id in $ids) {
        $raw = Get-SmartRaw $attrs $id
        if ($null -ne $raw) { return [uint64]$raw }
    }
    return $null
}

function Get-EnduranceUsedEstimate([object[]]$attrs) {
    # Prefer life-left normalized values (CDI-style core path): 231 / 233 / 202
    $lifeLeft = Get-SmartCurrent $attrs 231
    if ($null -eq $lifeLeft) { $lifeLeft = Get-SmartCurrent $attrs 233 }
    if ($null -eq $lifeLeft) { $lifeLeft = Get-SmartCurrent $attrs 202 }
    if ($null -ne $lifeLeft -and $lifeLeft -gt 0 -and $lifeLeft -lt 100) {
        return [double]([Math]::Max([Math]::Min(100 - [double]$lifeLeft, 100), 0))
    }

    # Vendor-specific fallback: some SSDs expose "used %" in RAW 202.
    $rawUsed = Get-SmartRaw $attrs 202
    if ($null -ne $rawUsed -and $rawUsed -gt 0 -and $rawUsed -le 100) {
        return [double]$rawUsed
    }

    return $null
}

function Get-TemperatureFromRaw([object]$raw) {
    if ($null -eq $raw) { return $null }
    try {
        $value = [uint64]$raw
    } catch {
        return $null
    }
    if ($value -le 200) { return [double]$value }
    return [double]($value -band 0xFF)
}

$disks = Get-Disk | Sort-Object Number
$physical = Get-PhysicalDisk
$win32 = Get-CimInstance Win32_DiskDrive
$physicalMedia = Get-CimInstance Win32_PhysicalMedia
$fpStatusAll = Get-WmiObject -Namespace root\wmi -Class MSStorageDriver_FailurePredictStatus
$fpDataAll = Get-WmiObject -Namespace root\wmi -Class MSStorageDriver_FailurePredictData
$fpThresholdAll = Get-WmiObject -Namespace root\wmi -Class MSStorageDriver_FailurePredictThresholds
$systemDiskNumber = $null
try {
    $os = Get-CimInstance Win32_OperatingSystem -ErrorAction Stop
    $systemDrive = if ($os) { [string]$os.SystemDrive } else { '' }
    if ($systemDrive) {
        $driveLetter = $systemDrive.Trim().TrimEnd('\').TrimEnd(':')
        if ($driveLetter) {
            $sysPart = Get-Partition -DriveLetter $driveLetter -ErrorAction SilentlyContinue | Select-Object -First 1
            if ($sysPart -and $null -ne $sysPart.DiskNumber) {
                $systemDiskNumber = [int]$sysPart.DiskNumber
            }
        }
    }
} catch {}
$result = @()

foreach ($d in $disks) {
    $wd = $win32 | Where-Object { $_.Index -eq $d.Number } | Select-Object -First 1
    $uniqueId = Normalize-Text $d.UniqueId
    $model = if ($wd) { Normalize-Text $wd.Model } else { '' }
    if (-not $model) { $model = Normalize-Text $d.FriendlyName }
    $firmware = if ($wd) { Normalize-Text $wd.FirmwareRevision } else { '' }
    $busTypeName = Get-BusTypeName $d.BusType
    $pnp = if ($wd) { Normalize-Text $wd.PNPDeviceID } else { '' }
    $usbVid = ''
    $usbPid = ''
    if ($pnp -match 'VID_([0-9A-Fa-f]{4})') { $usbVid = $Matches[1].ToUpper() }
    if ($pnp -match 'PID_([0-9A-Fa-f]{4})') { $usbPid = $Matches[1].ToUpper() }
    $interfaceType = if ($wd) { Normalize-Text $wd.InterfaceType } else { '' }
    $isUsb = Is-UsbDevice $busTypeName $interfaceType $pnp
    $transport = switch -Regex ($busTypeName) {
        '^NVMe$' { 'NVMe' }
        '^SATA$|^ATA$' { 'SATA/ATA' }
        '^USB$' { 'USB' }
        '^SAS$' { 'SAS' }
        '^RAID$' { 'RAID' }
        '^SD$' { 'SD' }
        default { if ($isUsb) { 'USB' } else { $busTypeName } }
    }
    $iface = if ($transport -eq 'NVMe') { 'NVMExpress' } elseif ($isUsb) { 'USB' } elseif ($interfaceType) { $interfaceType } else { $busTypeName }

    $pd = $physical | Where-Object {
        ($_.DeviceId -eq $d.Number) -or
        ($_.FriendlyName -eq $d.FriendlyName)
    } | Select-Object -First 1

    if (-not $pd) {
        $pd = $physical | Where-Object { [uint64]$_.Size -eq [uint64]$d.Size } | Select-Object -First 1
    }

    $pm = $null
    if ($wd -and $wd.DeviceID) {
        $wdDeviceId = Normalize-Text $wd.DeviceID
        $pm = $physicalMedia | Where-Object { (Normalize-Text $_.Tag) -eq $wdDeviceId } | Select-Object -First 1
    }

    $serialCandidates = @(
        $(if ($wd) { Normalize-Text $wd.SerialNumber } else { '' }),
        $(Normalize-Text $d.SerialNumber),
        $(if ($pd) { Normalize-Text $pd.SerialNumber } else { '' }),
        $(if ($pm) { Normalize-Text $pm.SerialNumber } else { '' })
    )
    $serial = Get-FirstValidSerial $serialCandidates
    $serialMasked = $false
    if (-not $serial) {
        $serialMasked = [bool](($serialCandidates | Where-Object { $_ -and (Test-MaskedSerial $_) } | Select-Object -First 1))
    }

    if ($pd) {
        if (-not $firmware) { $firmware = Normalize-Text $pd.FirmwareVersion }
        if (-not $model) { $model = Normalize-Text $pd.FriendlyName }
    }

    $media = Resolve-MediaType (Normalize-Text $d.MediaType) $pd $busTypeName $model $iface
    $health = if ($pd -and $pd.HealthStatus) { [string]$pd.HealthStatus } else { 'Unknown' }

    $reliability = $null
    if ($pd) {
        try {
            $reliability = Get-StorageReliabilityCounter -PhysicalDisk $pd -ErrorAction Stop
        } catch {}
    }

    $fpStatus = $null
    $fpData = $null
    $fpThresholds = $null
    if ($pnp) {
        $instance = $pnp.Replace('\', '_')
        $fpStatus = $fpStatusAll | Where-Object { $_.InstanceName -like "*$instance*" } | Select-Object -First 1
        $fpData = $fpDataAll | Where-Object { $_.InstanceName -like "*$instance*" } | Select-Object -First 1
        $fpThresholds = $fpThresholdAll | Where-Object { $_.InstanceName -like "*$instance*" } | Select-Object -First 1
    }

    $attrs = @()
    if ($fpData -and $fpData.VendorSpecific) {
        $thresholdBytes = if ($fpThresholds) { $fpThresholds.VendorSpecific } else { $null }
        $attrs = Parse-SmartAttributes -vendor $fpData.VendorSpecific -thresholds $thresholdBytes
    }

    $ataSmartAvailable = [bool]($attrs -and $attrs.Count -gt 0)
    $reliabilityAvailable = [bool]$reliability
    $smartSupported = $ataSmartAvailable -or $reliabilityAvailable -or [bool]$fpStatus
    $smartEnabled = if ($fpStatus) { [bool](-not $fpStatus.PredictFailure) } elseif ($smartSupported) { $true } else { $false }
    $smartSource = if ($ataSmartAvailable) { 'ATA_SMART_WMI' } elseif ($reliabilityAvailable) { 'STORAGE_RELIABILITY' } else { 'NONE' }

    $temperature = if ($reliability -and $null -ne $reliability.Temperature) { [double]$reliability.Temperature } else { $null }
    if ($null -eq $temperature) { $temperature = Get-TemperatureFromRaw (Get-SmartRaw $attrs 194) }
    if ($null -eq $temperature) { $temperature = Get-TemperatureFromRaw (Get-SmartRaw $attrs 190) }

    $powerOnHours = if ($reliability -and $null -ne $reliability.PowerOnHours) { [uint64]$reliability.PowerOnHours } else { Get-SmartRaw $attrs 9 }
    $powerCycleCount = if ($reliability -and $null -ne $reliability.PowerCycleCount) { [uint64]$reliability.PowerCycleCount } else { Get-SmartRaw $attrs 12 }

    $wear = if ($reliability -and $null -ne $reliability.Wear) { [double]$reliability.Wear } else { $null }
    $wearFromAttrs = Get-EnduranceUsedEstimate $attrs
    if ($null -eq $wear -or ($wear -le 0 -and $null -ne $wearFromAttrs -and $wearFromAttrs -gt 0)) {
        $wear = $wearFromAttrs
    }
    if ($null -ne $wear) {
        $wear = [double]([Math]::Max([Math]::Min([double]$wear, 100), 0))
    }
    if ($transport -ne 'NVMe' -and $null -ne $wear -and $wear -le 0 -and $null -eq $wearFromAttrs) {
        # Avoid showing misleading 100% health from placeholder Wear=0 on some SATA/USB paths.
        $wear = $null
    }

    $readErrorsTotal = if ($reliability -and $null -ne $reliability.ReadErrorsTotal) { [uint64]$reliability.ReadErrorsTotal } else { Get-FirstSmartRaw $attrs @(1, 187, 201) }
    $writeErrorsTotal = if ($reliability -and $null -ne $reliability.WriteErrorsTotal) { [uint64]$reliability.WriteErrorsTotal } else { Get-FirstSmartRaw $attrs @(200, 181, 171) }
    $hostWritesTotal = if ($reliability -and $null -ne $reliability.HostWritesTotal) { [uint64]$reliability.HostWritesTotal } else { Get-SmartRaw $attrs 241 }
    $hostReadsTotal = if ($reliability -and $null -ne $reliability.HostReadsTotal) { [uint64]$reliability.HostReadsTotal } else { Get-SmartRaw $attrs 242 }

    $reliabilityObj = $null
    if ($reliability) {
        $reliabilityObj = [PSCustomObject]@{
            Temperature = $reliability.Temperature
            PowerOnHours = $reliability.PowerOnHours
            PowerCycleCount = $reliability.PowerCycleCount
            Wear = $reliability.Wear
            ReadErrorsTotal = $reliability.ReadErrorsTotal
            WriteErrorsTotal = $reliability.WriteErrorsTotal
            ReadErrorsUncorrected = $reliability.ReadErrorsUncorrected
            WriteErrorsUncorrected = $reliability.WriteErrorsUncorrected
            HostReadsTotal = $reliability.HostReadsTotal
            HostWritesTotal = $reliability.HostWritesTotal
        }
    }

    $notes = @()
    if ($ataSmartAvailable -and $reliabilityAvailable) {
        $notes += 'Using ATA SMART attributes and Storage Reliability counters.'
    } elseif ($ataSmartAvailable) {
        $notes += 'Using ATA SMART attribute table.'
    } elseif ($reliabilityAvailable) {
        $notes += 'ATA SMART attribute table unavailable; using Storage Reliability counters.'
    } else {
        $notes += 'SMART/reliability counters unavailable for this device path (common with some USB bridges/RAID drivers).'
    }
    if ($serialMasked) {
        $notes += 'Serial number appears masked by controller/driver; another identifier may be required for exact model matching.'
    } elseif (-not $serial) {
        $notes += 'No usable serial number returned by current Windows APIs for this device.'
    }
    if ($ataSmartAvailable -and -not $reliabilityAvailable) {
        $notes += 'Some counters were derived from ATA SMART attributes because Storage Reliability counters were incomplete.'
    }
    if ($isUsb -and -not $ataSmartAvailable -and -not $reliabilityAvailable) {
        $notes += 'USB bridge may block pass-through SMART commands on this enclosure.'
    }
    $isSystemDisk = if ($null -ne $systemDiskNumber) { [int]$d.Number -eq $systemDiskNumber } else { [bool]($d.IsBoot -or $d.IsSystem) }

    $result += [PSCustomObject]@{
        id = "disk$($d.Number)"
        disk_number = [int]$d.Number
        model = $model
        friendly_name = [string]$d.FriendlyName
        serial_number = $serial
        firmware_version = $firmware
        interface_type = $iface
        pnp_device_id = $pnp
        usb_vendor_id = $usbVid
        usb_product_id = $usbPid
        transport_type = $transport
        is_usb = [bool]$isUsb
        bus_type = $busTypeName
        unique_id = $uniqueId
        media_type = $media
        size_bytes = [uint64]$d.Size
        is_system = [bool]$isSystemDisk
        health_status = $health
        smart_supported = $smartSupported
        smart_enabled = $smartEnabled
        smart_data_source = $smartSource
        ata_smart_available = [bool]$ataSmartAvailable
        reliability_available = [bool]$reliabilityAvailable
        temperature_c = $temperature
        power_on_hours = $powerOnHours
        power_cycle_count = $powerCycleCount
        percentage_used = $wear
        read_errors_total = $readErrorsTotal
        write_errors_total = $writeErrorsTotal
        host_reads_total = $hostReadsTotal
        host_writes_total = $hostWritesTotal
        smart_attributes = $attrs
        reliability = $reliabilityObj
        notes = $notes
    }
}

$result | ConvertTo-Json -Depth 8
"#;

/// List all disks on Windows using PowerShell
pub async fn list_disks() -> Result<Vec<DiskInfo>> {
    let output = CommandExecutor::execute_allow_fail(
        "powershell.exe",
        &["-NoProfile", "-Command", PS_LIST_DISKS],
    )?;

    let trimmed = output.trim();

    if trimmed.is_empty() {
        return Ok(vec![]);
    }

    // Find the JSON portion (skip any non-JSON output before it)
    let json_start = trimmed.find('[').or_else(|| trimmed.find('{'));
    let json_str = match json_start {
        Some(pos) => &trimmed[pos..],
        None => {
            info!(
                "No JSON found in PowerShell output: {}",
                &trimmed[..trimmed.len().min(200)]
            );
            return Ok(vec![]);
        }
    };

    let disks: Vec<DiskInfo> = if json_str.starts_with('[') {
        let raw: Vec<serde_json::Value> = serde_json::from_str(json_str).map_err(|e| {
            AppError::JsonError(format!("{}: {}", e, &json_str[..json_str.len().min(300)]))
        })?;
        raw.iter().map(|d| parse_disk(d)).collect()
    } else {
        let raw: serde_json::Value = serde_json::from_str(json_str).map_err(|e| {
            AppError::JsonError(format!("{}: {}", e, &json_str[..json_str.len().min(300)]))
        })?;
        vec![parse_disk(&raw)]
    };

    info!("Found {} disks", disks.len());
    Ok(disks)
}

/// List detailed disk diagnostics on Windows
pub async fn list_disk_diagnostics() -> Result<Vec<DiskDiagnostics>> {
    let output = CommandExecutor::execute_allow_fail(
        "powershell.exe",
        &["-NoProfile", "-Command", PS_LIST_DISK_DIAGNOSTICS],
    )?;

    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Ok(vec![]);
    }

    let json_start = trimmed.find('[').or_else(|| trimmed.find('{'));
    let json_str = match json_start {
        Some(pos) => &trimmed[pos..],
        None => {
            info!(
                "No diagnostics JSON found in PowerShell output: {}",
                &trimmed[..trimmed.len().min(300)]
            );
            return Ok(vec![]);
        }
    };

    let mut diagnostics: Vec<DiskDiagnostics> = if json_str.starts_with('[') {
        serde_json::from_str(json_str).map_err(|e| {
            AppError::JsonError(format!("{}: {}", e, &json_str[..json_str.len().min(600)]))
        })?
    } else {
        vec![serde_json::from_str(json_str).map_err(|e| {
            AppError::JsonError(format!("{}: {}", e, &json_str[..json_str.len().min(600)]))
        })?]
    };

    enrich_with_smartctl(&mut diagnostics);
    enrich_with_native_smart(&mut diagnostics);
    normalize_endurance_percentage(&mut diagnostics);
    Ok(diagnostics)
}

fn enrich_with_native_smart(diagnostics: &mut [DiskDiagnostics]) {
    for diag in diagnostics.iter_mut() {
        let looks_nvme = is_nvme_diag(diag);
        let mut nvme_ok = false;
        let mut ata_ok = false;

        if looks_nvme {
            nvme_ok = try_enrich_native_nvme(diag);
        }

        if nvme_ok {
            continue;
        }

        match smart::DiskHandle::open(diag.disk_number) {
            Ok(handle) => match handle.read_smart_data() {
                Ok(smart_data) => {
                    info!(
                        "Successfully read ATA SMART data for disk {}",
                        diag.disk_number
                    );

                    if diag.temperature_c.is_none() {
                        diag.temperature_c = smart_data.temperature.map(|t| t as f64);
                    }
                    if diag.power_on_hours.is_none() {
                        diag.power_on_hours = smart_data.power_on_hours;
                    }
                    if diag.power_cycle_count.is_none() {
                        diag.power_cycle_count = smart_data.power_cycle_count;
                    }

                    let attrs: Vec<SmartAttribute> = smart_data
                        .attributes
                        .iter()
                        .map(|attr| SmartAttribute {
                            id: attr.id as u32,
                            name: get_smart_attribute_name(attr.id),
                            current: Some(attr.current as u32),
                            worst: Some(attr.worst as u32),
                            threshold: if smart_data.thresholds_available {
                                Some(attr.threshold as u32)
                            } else {
                                None
                            },
                            raw: Some(attr.raw),
                            raw_hex: format!("0x{:012X}", attr.raw),
                        })
                        .collect();

                    if !attrs.is_empty() && attrs.len() >= diag.smart_attributes.len() {
                        diag.smart_attributes = attrs;
                        diag.ata_smart_available = true;
                        diag.smart_supported = true;
                        let (native_code, native_note) = match smart_data.read_method {
                            smart::SmartReadMethod::AtaPassThrough => (
                                "ATA_NATIVE_IOCTL",
                                "ATA SMART data read directly via Windows IOCTL (native API).",
                            ),
                            smart::SmartReadMethod::PhysicalDrive => (
                                "ATA_NATIVE_DFP",
                                "ATA SMART data read via legacy SMART DFP command path.",
                            ),
                            smart::SmartReadMethod::SatBridge => (
                                "ATA_NATIVE_SAT",
                                "ATA SMART data read via SAT bridge fallback (SCSI pass-through).",
                            ),
                        };
                        diag.smart_data_source =
                            merge_smart_source(&diag.smart_data_source, native_code);
                        add_note_unique(diag, native_note);
                        if !smart_data.thresholds_available {
                            add_note_unique(
                                diag,
                                "SMART threshold table was not returned by device/bridge; threshold values are unavailable.",
                            );
                        }
                    }
                    if let Some(used) = derive_endurance_used_from_attrs(&diag.smart_attributes) {
                        if diag.percentage_used.is_none()
                            || diag.percentage_used.unwrap_or(0.0) <= 0.0
                        {
                            diag.percentage_used = Some(used);
                        }
                    }
                    ata_ok = true;
                }
                Err(e) => {
                    warn!(
                        "Failed to read ATA SMART data for disk {}: {}",
                        diag.disk_number, e
                    );
                }
            },
            Err(e) => {
                warn!(
                    "Failed to open disk {} for ATA SMART reading: {}",
                    diag.disk_number, e
                );
            }
        }

        // CrystalDiskInfo also has many fallback probing paths; as a lightweight step we try
        // NVMe Storage Query again when ATA path failed, for misreported bridge/controller cases.
        if !looks_nvme && !ata_ok {
            let _ = try_enrich_native_nvme(diag);
        }
    }
}

fn is_nvme_diag(diag: &DiskDiagnostics) -> bool {
    let haystack = format!(
        "{} {} {} {} {}",
        diag.transport_type, diag.interface_type, diag.bus_type, diag.model, diag.pnp_device_id
    )
    .to_ascii_uppercase();

    haystack.contains("NVME") || haystack.contains("NVMEXPRESS") || haystack.contains("OPTANE")
}

fn try_enrich_native_nvme(diag: &mut DiskDiagnostics) -> bool {
    match smart::nvme::NVMeHandle::open(diag.disk_number) {
        Ok(handle) => {
            if let Ok(id_ctrl) = handle.read_identify_controller() {
                if diag.model.trim().is_empty() && !id_ctrl.model.is_empty() {
                    diag.model = id_ctrl.model;
                }
                if (diag.serial_number.trim().is_empty() || is_masked_serial(&diag.serial_number))
                    && !id_ctrl.serial_number.is_empty()
                {
                    diag.serial_number = id_ctrl.serial_number;
                }
                if diag.firmware_version.trim().is_empty() && !id_ctrl.firmware_version.is_empty() {
                    diag.firmware_version = id_ctrl.firmware_version;
                }
            }

            match handle.read_smart_data() {
                Ok(nvme_data) => {
                    info!(
                        "Successfully read NVMe SMART data for disk {}",
                        diag.disk_number
                    );
                    apply_native_nvme_data(diag, &nvme_data);
                    add_note_unique(
                        diag,
                        "NVMe SMART data read directly via Windows Storage Query API.",
                    );
                    true
                }
                Err(e) => {
                    warn!(
                        "Failed to read NVMe SMART data for disk {}: {}",
                        diag.disk_number, e
                    );
                    false
                }
            }
        }
        Err(e) => {
            warn!(
                "Failed to open NVMe disk {} for SMART reading: {}",
                diag.disk_number, e
            );
            false
        }
    }
}

fn apply_native_nvme_data(diag: &mut DiskDiagnostics, nvme_data: &smart::nvme::NVMeSmartData) {
    diag.transport_type = "NVMe".to_string();
    diag.interface_type = "NVMExpress".to_string();
    diag.media_type = "SSD".to_string();

    if diag.temperature_c.is_none() && nvme_data.temperature > -1000 {
        diag.temperature_c = Some(nvme_data.temperature as f64);
    }
    if diag.power_on_hours.is_none() {
        diag.power_on_hours = Some(u128_to_u64(nvme_data.power_on_hours));
    }
    if diag.power_cycle_count.is_none() {
        diag.power_cycle_count = Some(u128_to_u64(nvme_data.power_cycles));
    }
    if diag.percentage_used.is_none() {
        diag.percentage_used = Some((nvme_data.percentage_used as f64).clamp(0.0, 100.0));
    }
    if diag.host_reads_total.is_none() {
        let units = u128_to_u64(nvme_data.data_units_read);
        let cmds = u128_to_u64(nvme_data.host_read_commands);
        diag.host_reads_total = Some(if units > 0 { units } else { cmds });
    }
    if diag.host_writes_total.is_none() {
        let units = u128_to_u64(nvme_data.data_units_written);
        let cmds = u128_to_u64(nvme_data.host_write_commands);
        diag.host_writes_total = Some(if units > 0 { units } else { cmds });
    }
    if diag.read_errors_total.is_none() {
        diag.read_errors_total = Some(u128_to_u64(nvme_data.media_errors));
    }
    if diag.write_errors_total.is_none() {
        diag.write_errors_total = Some(u128_to_u64(nvme_data.num_err_log_entries));
    }

    diag.smart_supported = true;
    diag.smart_enabled = true;
    diag.smart_data_source = merge_smart_source(&diag.smart_data_source, "NVME_NATIVE_IOCTL");

    let mut rel = match std::mem::take(&mut diag.reliability) {
        Value::Object(map) => map,
        _ => Map::new(),
    };

    insert_if_absent(
        &mut rel,
        "NvmeIoctl.CriticalWarning",
        Some(Value::from(nvme_data.critical_warning)),
    );
    insert_if_absent(
        &mut rel,
        "NvmeIoctl.AvailableSpare",
        Some(Value::from(nvme_data.available_spare)),
    );
    insert_if_absent(
        &mut rel,
        "NvmeIoctl.AvailableSpareThreshold",
        Some(Value::from(nvme_data.available_spare_threshold)),
    );
    insert_if_absent(
        &mut rel,
        "NvmeIoctl.PercentageUsed",
        Some(Value::from(nvme_data.percentage_used)),
    );
    insert_if_absent(
        &mut rel,
        "NvmeIoctl.DataUnitsRead",
        Some(u128_to_json_value(nvme_data.data_units_read)),
    );
    insert_if_absent(
        &mut rel,
        "NvmeIoctl.DataUnitsWritten",
        Some(u128_to_json_value(nvme_data.data_units_written)),
    );
    insert_if_absent(
        &mut rel,
        "NvmeIoctl.HostReadCommands",
        Some(u128_to_json_value(nvme_data.host_read_commands)),
    );
    insert_if_absent(
        &mut rel,
        "NvmeIoctl.HostWriteCommands",
        Some(u128_to_json_value(nvme_data.host_write_commands)),
    );
    insert_if_absent(
        &mut rel,
        "NvmeIoctl.ControllerBusyTime",
        Some(u128_to_json_value(nvme_data.controller_busy_time)),
    );
    insert_if_absent(
        &mut rel,
        "NvmeIoctl.PowerCycles",
        Some(u128_to_json_value(nvme_data.power_cycles)),
    );
    insert_if_absent(
        &mut rel,
        "NvmeIoctl.PowerOnHours",
        Some(u128_to_json_value(nvme_data.power_on_hours)),
    );
    insert_if_absent(
        &mut rel,
        "NvmeIoctl.UnsafeShutdowns",
        Some(u128_to_json_value(nvme_data.unsafe_shutdowns)),
    );
    insert_if_absent(
        &mut rel,
        "NvmeIoctl.MediaErrors",
        Some(u128_to_json_value(nvme_data.media_errors)),
    );
    insert_if_absent(
        &mut rel,
        "NvmeIoctl.ErrorLogEntries",
        Some(u128_to_json_value(nvme_data.num_err_log_entries)),
    );
    insert_if_absent(
        &mut rel,
        "NvmeIoctl.WarningTempTime",
        Some(Value::from(nvme_data.warning_temp_time)),
    );
    insert_if_absent(
        &mut rel,
        "NvmeIoctl.CriticalTempTime",
        Some(Value::from(nvme_data.critical_temp_time)),
    );
    insert_if_absent(
        &mut rel,
        "NvmeIoctl.TemperatureSensors",
        Some(Value::Array(
            nvme_data
                .temp_sensors
                .iter()
                .map(|v| Value::from(*v))
                .collect(),
        )),
    );

    diag.reliability = Value::Object(rel);
}

fn u128_to_u64(value: u128) -> u64 {
    value.min(u64::MAX as u128) as u64
}

fn u128_to_json_value(value: u128) -> Value {
    if value <= u64::MAX as u128 {
        Value::from(value as u64)
    } else {
        Value::String(value.to_string())
    }
}

fn get_smart_attribute_name(id: u8) -> String {
    match id {
        1 => "Read Error Rate".to_string(),
        2 => "Throughput Performance".to_string(),
        3 => "Spin-Up Time".to_string(),
        4 => "Start/Stop Count".to_string(),
        5 => "Reallocated Sectors Count".to_string(),
        7 => "Seek Error Rate".to_string(),
        8 => "Seek Time Performance".to_string(),
        9 => "Power-On Hours".to_string(),
        10 => "Spin Retry Count".to_string(),
        11 => "Calibration Retry Count".to_string(),
        12 => "Power Cycle Count".to_string(),
        170 => "Available Reserved Space".to_string(),
        171 => "Program Fail Count".to_string(),
        172 => "Erase Fail Count".to_string(),
        173 => "Wear Leveling Count".to_string(),
        174 => "Unexpected Power Loss Count".to_string(),
        177 => "Wear Range Delta".to_string(),
        179 => "Used Reserved Block Count Total".to_string(),
        180 => "Unused Reserved Block Count Total".to_string(),
        181 => "Program Fail Count Total".to_string(),
        182 => "Erase Fail Count".to_string(),
        183 => "Runtime Bad Block".to_string(),
        184 => "End-to-End Error".to_string(),
        187 => "Reported Uncorrectable Errors".to_string(),
        188 => "Command Timeout".to_string(),
        190 => "Airflow Temperature".to_string(),
        191 => "G-Sense Error Rate".to_string(),
        192 => "Power-Off Retract Count".to_string(),
        193 => "Load/Unload Cycle Count".to_string(),
        194 => "Temperature".to_string(),
        195 => "Hardware ECC Recovered".to_string(),
        196 => "Reallocation Event Count".to_string(),
        197 => "Current Pending Sector Count".to_string(),
        198 => "Offline Uncorrectable Sector Count".to_string(),
        199 => "UltraDMA CRC Error Count".to_string(),
        200 => "Write Error Rate".to_string(),
        201 => "Soft Read Error Rate".to_string(),
        202 => "Data Address Mark Errors".to_string(),
        206 => "Flying Height".to_string(),
        210 => "Vibration During Write".to_string(),
        211 => "Vibration During Write Time".to_string(),
        212 => "Shock During Write".to_string(),
        220 => "Disk Shift".to_string(),
        222 => "Loaded Hours".to_string(),
        223 => "Load/Unload Retry Count".to_string(),
        224 => "Load Friction".to_string(),
        225 => "Load/Unload Cycle Count".to_string(),
        226 => "Load-in Time".to_string(),
        227 => "Torque Amplification Count".to_string(),
        228 => "Power-Off Retract Cycle".to_string(),
        230 => "Drive Life Protection Status".to_string(),
        231 => "SSD Life Left".to_string(),
        232 => "Available Reserved Space".to_string(),
        233 => "Media Wearout Indicator".to_string(),
        234 => "Average Erase Count".to_string(),
        235 => "Good Block Count".to_string(),
        241 => "Total LBAs Written".to_string(),
        242 => "Total LBAs Read".to_string(),
        243 => "Total LBAs Written Expanded".to_string(),
        244 => "Total LBAs Read Expanded".to_string(),
        245 => "NAND Writes (1GiB)".to_string(),
        246 => "Total NAND Writes".to_string(),
        247 => "Host Program NAND Pages Count".to_string(),
        248 => "FTL Program NAND Pages Count".to_string(),
        249 => "NAND Writes (1GiB)".to_string(),
        250 => "Read Error Retry Rate".to_string(),
        251 => "Minimum Spares Remaining".to_string(),
        252 => "Newly Added Bad Flash Block".to_string(),
        254 => "Free Fall Protection".to_string(),
        _ => format!("Attribute {}", id),
    }
}

fn enrich_with_smartctl(diagnostics: &mut [DiskDiagnostics]) {
    if diagnostics.is_empty() {
        return;
    }

    if !smartctl_installed() {
        for diag in diagnostics.iter_mut() {
            add_note_unique(
                diag,
                "smartctl not found in bundled resources or PATH; include smartmontools to enable extended SMART details.",
            );
        }
        return;
    }

    let scan_entries = smartctl_scan_entries();
    for diag in diagnostics.iter_mut() {
        if let Some(payload) = get_smartctl_payload_for_disk(diag, scan_entries.as_deref()) {
            apply_smartctl_payload(diag, &payload);
            add_note_unique(diag, "Extended SMART details were enhanced via smartctl.");
        }
    }
}

fn smartctl_installed() -> bool {
    run_smartctl_allow_fail(&["--version"]).is_some()
}

#[derive(Debug, Clone)]
struct SmartctlScanEntry {
    name: String,
    device_type: Option<String>,
    info_name: String,
}

fn smartctl_scan_entries() -> Option<Vec<SmartctlScanEntry>> {
    let scan_cmds: [&[&str]; 4] = [
        &["--scan-open", "-j"],
        &["--scan", "-j"],
        &["--scan-open"],
        &["--scan"],
    ];

    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    for args in scan_cmds {
        let Some(output) = run_smartctl_allow_fail(args) else {
            continue;
        };

        let parsed = if args.contains(&"-j") {
            extract_json_value(&output)
                .map(|payload| parse_smartctl_scan_entries_json(&payload))
                .unwrap_or_default()
        } else {
            parse_smartctl_scan_entries_text(&output)
        };

        for entry in parsed {
            let key = format!(
                "{}\u{1F}{}",
                entry.name.to_ascii_lowercase(),
                entry
                    .device_type
                    .clone()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
            );
            if seen.insert(key) {
                entries.push(entry);
            }
        }
    }

    if entries.is_empty() {
        None
    } else {
        Some(entries)
    }
}

fn parse_smartctl_scan_entries_json(payload: &Value) -> Vec<SmartctlScanEntry> {
    let mut entries = Vec::new();
    let Some(devices) = payload.get("devices").and_then(Value::as_array) else {
        return entries;
    };

    for dev in devices {
        let name = dev
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        if name.is_empty() {
            continue;
        }
        let info_name = dev
            .get("info_name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let device_type = dev
            .get("type")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned);

        entries.push(SmartctlScanEntry {
            name,
            device_type,
            info_name,
        });
    }
    entries
}

fn parse_smartctl_scan_entries_text(output: &str) -> Vec<SmartctlScanEntry> {
    let mut entries = Vec::new();
    let scan_regex = Regex::new(
        r#"(?i)^\s*(?P<name>\S+)(?:\s+-d\s+(?P<dtype>[^\s#]+))?(?:\s+#\s*(?P<info>.*))?$"#,
    )
    .ok();

    for raw_line in output.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(re) = scan_regex.as_ref() {
            if let Some(caps) = re.captures(line) {
                let name = caps
                    .name("name")
                    .map(|m| m.as_str().trim().to_string())
                    .unwrap_or_default();
                if name.is_empty() || !name.starts_with('/') {
                    continue;
                }
                let device_type = caps
                    .name("dtype")
                    .map(|m| m.as_str().trim().to_string())
                    .filter(|s| !s.is_empty());
                let info_name = caps
                    .name("info")
                    .map(|m| m.as_str().trim().to_string())
                    .unwrap_or_default();

                entries.push(SmartctlScanEntry {
                    name,
                    device_type,
                    info_name,
                });
            }
        }
    }

    entries
}

fn smartctl_entry_matches_disk(entry: &SmartctlScanEntry, disk_number: u32) -> bool {
    let key_pd = format!("pd{}", disk_number).to_ascii_lowercase();
    let key_phy = format!("physicaldrive{}", disk_number).to_ascii_lowercase();
    let key_disk = format!("disk{}", disk_number).to_ascii_lowercase();
    let name = entry.name.to_ascii_lowercase();
    let info = entry.info_name.to_ascii_lowercase();

    [name, info]
        .iter()
        .any(|v| v.contains(&key_pd) || v.contains(&key_phy) || v.contains(&key_disk))
}

fn push_smartctl_attempt(
    attempts: &mut Vec<Vec<String>>,
    seen: &mut HashSet<String>,
    device: &str,
    device_type: Option<&str>,
) {
    let mut args = vec!["-x".to_string(), "-j".to_string()];
    if let Some(dt) = device_type.map(str::trim).filter(|dt| !dt.is_empty()) {
        args.push("-d".to_string());
        args.push(dt.to_string());
    }
    args.push(device.to_string());

    let key = args.join("\u{1F}");
    if seen.insert(key) {
        attempts.push(args);
    }
}

fn normalized_eq(a: &str, b: &str) -> bool {
    let na: String = a
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let nb: String = b
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    !na.is_empty() && !nb.is_empty() && na == nb
}

fn payload_matches_diag(payload: &Value, diag: &DiskDiagnostics) -> bool {
    let pd_name = format!("/dev/pd{}", diag.disk_number).to_ascii_lowercase();
    if get_string_path(payload, &["device", "name"])
        .map(|n| n.to_ascii_lowercase().contains(&pd_name))
        .unwrap_or(false)
    {
        return true;
    }

    let payload_serial = get_string_path(payload, &["serial_number"]).unwrap_or_default();
    let diag_serial = diag.serial_number.trim();
    if !diag_serial.is_empty()
        && !is_masked_serial(diag_serial)
        && !payload_serial.is_empty()
        && normalized_eq(diag_serial, &payload_serial)
    {
        return true;
    }

    let payload_model = get_string_path(payload, &["model_name"]).unwrap_or_default();
    let diag_model = if diag.model.trim().is_empty() {
        diag.friendly_name.trim()
    } else {
        diag.model.trim()
    };
    let model_like = !payload_model.is_empty()
        && !diag_model.is_empty()
        && (payload_model
            .to_ascii_uppercase()
            .contains(&diag_model.to_ascii_uppercase())
            || diag_model
                .to_ascii_uppercase()
                .contains(&payload_model.to_ascii_uppercase()));

    if model_like {
        if let Some(cap) = get_u64_path(payload, &["user_capacity", "bytes"]) {
            let size = diag.size_bytes;
            if size > 0 {
                let diff = cap.abs_diff(size);
                let tolerance = (size / 20).max(64 * 1024 * 1024);
                if diff <= tolerance {
                    return true;
                }
            }
        }
    }

    false
}

fn get_smartctl_payload_for_disk(
    diag: &DiskDiagnostics,
    scan_entries: Option<&[SmartctlScanEntry]>,
) -> Option<Value> {
    let device = format!("/dev/pd{}", diag.disk_number);
    let mut attempts: Vec<Vec<String>> = Vec::new();
    let mut seen = HashSet::new();

    push_smartctl_attempt(&mut attempts, &mut seen, &device, None);

    let is_nvme = diag.transport_type.eq_ignore_ascii_case("nvme")
        || diag.interface_type.eq_ignore_ascii_case("nvmexpress")
        || diag.bus_type.eq_ignore_ascii_case("nvme");

    if diag.is_usb {
        push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("sat,auto"));
        push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("sat"));
        push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("sat,12"));
        push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("sat,16"));
        push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("scsi"));
    }

    if is_nvme {
        push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("nvme"));
        if diag.is_usb {
            // CrystalDiskInfo has dedicated NVMe-over-USB paths for JMicron/ASMedia/Realtek.
            push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("sntjmicron"));
            push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("sntjmicron,0"));
            push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("sntjmicron,1"));
            push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("sntasmedia"));
            push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("sntrealtek"));
            push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("sntrealtek,0"));
            push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("sntrealtek,1"));
        }
    } else {
        if diag.is_usb {
            push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("usbjmicron"));
        }
        push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("sat"));
        push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("scsi"));
    }

    let usb_vid = diag.usb_vendor_id.trim().to_ascii_uppercase();
    if !usb_vid.is_empty() {
        match usb_vid.as_str() {
            // JMicron
            "152D" => {
                push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("usbjmicron"));
                push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("sntjmicron"));
                push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("sntjmicron,0"));
                push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("sntjmicron,1"));
            }
            // ASMedia
            "174C" => {
                push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("sntasmedia"));
            }
            // Realtek
            "0BDA" => {
                push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("sntrealtek"));
                push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("sntrealtek,0"));
                push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("sntrealtek,1"));
            }
            // Cypress / Prolific / Sunplus
            "04B4" => push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("usbcypress")),
            "067B" => push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("usbprolific")),
            "04FC" => push_smartctl_attempt(&mut attempts, &mut seen, &device, Some("usbsunplus")),
            _ => {}
        }
    }

    if let Some(entries) = scan_entries {
        for entry in entries
            .iter()
            .filter(|e| smartctl_entry_matches_disk(e, diag.disk_number))
        {
            push_smartctl_attempt(
                &mut attempts,
                &mut seen,
                &entry.name,
                entry.device_type.as_deref(),
            );
        }
    }

    for args in attempts {
        if let Some(payload) = run_smartctl_json(&args) {
            if is_useful_smartctl_payload(&payload) {
                return Some(payload);
            }
        }
    }

    if let Some(entries) = scan_entries {
        // Fallback: probe scanned entries and match by serial/model/size
        for entry in entries {
            let mut fallback = vec!["-x".to_string(), "-j".to_string()];
            if let Some(dt) = entry
                .device_type
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                fallback.push("-d".to_string());
                fallback.push(dt.to_string());
            }
            fallback.push(entry.name.clone());

            if let Some(payload) = run_smartctl_json(&fallback) {
                if is_useful_smartctl_payload(&payload) && payload_matches_diag(&payload, diag) {
                    return Some(payload);
                }
            }
        }
    }

    None
}

fn run_smartctl_json(args: &[String]) -> Option<Value> {
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = run_smartctl_allow_fail(&arg_refs)?;
    extract_json_value(&output)
}

fn run_smartctl_allow_fail(args: &[&str]) -> Option<String> {
    for cmd in smartctl_candidates() {
        if let Ok(output) = CommandExecutor::execute_allow_fail(&cmd, args) {
            return Some(output);
        }
    }
    None
}

fn smartctl_candidates() -> Vec<String> {
    let mut candidates = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            // Prefer bundled smartmontools shipped with the app.
            push_candidate_path(
                &mut candidates,
                dir.join("smartmontools").join("bin").join("smartctl.exe"),
            );
            push_candidate_path(
                &mut candidates,
                dir.join("resources")
                    .join("smartmontools")
                    .join("bin")
                    .join("smartctl.exe"),
            );
            push_candidate_path(
                &mut candidates,
                dir.join("..")
                    .join("resources")
                    .join("smartmontools")
                    .join("bin")
                    .join("smartctl.exe"),
            );

            // Backward compatibility for old bundled layout.
            push_candidate_path(
                &mut candidates,
                dir.join("resources").join("smartctl").join("smartctl.exe"),
            );
            push_candidate_path(
                &mut candidates,
                dir.join("..")
                    .join("resources")
                    .join("smartctl")
                    .join("smartctl.exe"),
            );
            push_candidate_path(&mut candidates, dir.join("smartctl.exe"));
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        // Repository/dev layout convenience.
        push_candidate_path(
            &mut candidates,
            cwd.join("src-tauri")
                .join("resources")
                .join("smartmontools")
                .join("bin")
                .join("smartctl.exe"),
        );
        push_candidate_path(
            &mut candidates,
            cwd.join("smartmontools").join("bin").join("smartctl.exe"),
        );
        push_candidate_path(
            &mut candidates,
            cwd.join("useable_software")
                .join("smartmontools")
                .join("bin")
                .join("smartctl.exe"),
        );
    }

    // PATH and global installs as fallback.
    candidates.extend([
        "smartctl".to_string(),
        "smartctl.exe".to_string(),
        r"C:\Program Files\smartmontools\bin\smartctl.exe".to_string(),
        r"C:\Program Files (x86)\smartmontools\bin\smartctl.exe".to_string(),
    ]);

    // Keep only existing absolute/relative paths; retain command names.
    let mut filtered = Vec::new();
    for c in candidates {
        if c.contains('\\') || c.contains('/') {
            if std::path::Path::new(&c).exists() {
                filtered.push(c);
            }
        } else {
            filtered.push(c);
        }
    }

    let mut seen = std::collections::HashSet::new();
    filtered
        .into_iter()
        .filter(|s| seen.insert(s.to_ascii_lowercase()))
        .collect()
}

fn push_candidate_path(candidates: &mut Vec<String>, path: std::path::PathBuf) {
    candidates.push(path.to_string_lossy().to_string());
}

fn extract_json_value(output: &str) -> Option<Value> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return None;
    }
    let start = trimmed.find('{').or_else(|| trimmed.find('['))?;
    serde_json::from_str(&trimmed[start..]).ok()
}

fn is_useful_smartctl_payload(payload: &Value) -> bool {
    payload.get("smartctl").is_some()
        && (payload.get("model_name").is_some()
            || payload.get("serial_number").is_some()
            || payload.pointer("/ata_smart_attributes/table").is_some()
            || payload.get("nvme_smart_health_information_log").is_some()
            || payload.pointer("/temperature/current").is_some()
            || payload.pointer("/power_on_time/hours").is_some())
}

fn apply_smartctl_payload(diag: &mut DiskDiagnostics, payload: &Value) {
    if let Some(model) = get_string_path(payload, &["model_name"]).filter(|s| !s.is_empty()) {
        if diag.model.trim().is_empty() {
            diag.model = model;
        }
    }

    if let Some(firmware) =
        get_string_path(payload, &["firmware_version"]).filter(|s| !s.is_empty())
    {
        if diag.firmware_version.trim().is_empty() {
            diag.firmware_version = firmware;
        }
    }

    if let Some(serial) = get_string_path(payload, &["serial_number"]).filter(|s| !s.is_empty()) {
        if diag.serial_number.trim().is_empty() || is_masked_serial(&diag.serial_number) {
            diag.serial_number = serial;
        }
    }

    if let Some(protocol) = get_string_path(payload, &["device", "protocol"]) {
        if protocol.eq_ignore_ascii_case("nvme") {
            diag.transport_type = "NVMe".to_string();
            diag.interface_type = "NVMExpress".to_string();
            if diag.media_type.eq_ignore_ascii_case("unknown") || diag.media_type.is_empty() {
                diag.media_type = "SSD".to_string();
            }
        }
    }

    if let Some(rotation) = get_u64_path(payload, &["rotation_rate"]) {
        if rotation == 0 {
            diag.media_type = "SSD".to_string();
        } else if rotation > 0 {
            diag.media_type = "HDD".to_string();
        }
    }

    if diag.temperature_c.is_none() {
        diag.temperature_c = extract_smartctl_temperature(payload);
    }
    if diag.power_on_hours.is_none() {
        diag.power_on_hours = get_u64_path(payload, &["power_on_time", "hours"]).or_else(|| {
            get_u64_path(
                payload,
                &["nvme_smart_health_information_log", "power_on_hours"],
            )
        });
    }
    if diag.power_cycle_count.is_none() {
        diag.power_cycle_count = get_u64_path(payload, &["power_cycle_count"]).or_else(|| {
            get_u64_path(
                payload,
                &["nvme_smart_health_information_log", "power_cycles"],
            )
        });
    }
    if diag.percentage_used.is_none() {
        diag.percentage_used = get_f64_path(
            payload,
            &["nvme_smart_health_information_log", "percentage_used"],
        );
    }
    if diag.host_reads_total.is_none() {
        diag.host_reads_total = get_u64_path(
            payload,
            &["nvme_smart_health_information_log", "host_reads"],
        )
        .or_else(|| {
            get_u64_path(
                payload,
                &["nvme_smart_health_information_log", "data_units_read"],
            )
        });
    }
    if diag.host_writes_total.is_none() {
        diag.host_writes_total = get_u64_path(
            payload,
            &["nvme_smart_health_information_log", "host_writes"],
        )
        .or_else(|| {
            get_u64_path(
                payload,
                &["nvme_smart_health_information_log", "data_units_written"],
            )
        });
    }
    if diag.read_errors_total.is_none() {
        diag.read_errors_total = get_u64_path(
            payload,
            &["nvme_smart_health_information_log", "media_errors"],
        );
    }

    if diag.write_errors_total.is_none() {
        diag.write_errors_total = get_u64_path(
            payload,
            &["nvme_smart_health_information_log", "num_err_log_entries"],
        );
    }

    if let Some(enabled) = get_bool_path(payload, &["smart_support", "enabled"]) {
        diag.smart_enabled = enabled;
        diag.smart_supported = true;
    }
    if let Some(_passed) = get_bool_path(payload, &["smart_status", "passed"]) {
        diag.smart_supported = true;
    }

    let smartctl_attrs = parse_smartctl_ata_attributes(payload);
    if !smartctl_attrs.is_empty() {
        if diag.smart_attributes.len() < smartctl_attrs.len() {
            diag.smart_attributes = smartctl_attrs.clone();
        }
        diag.ata_smart_available = true;
        diag.smart_supported = true;
        diag.smart_data_source = merge_smart_source(&diag.smart_data_source, "SMARTCTL_ATA");

        if diag.temperature_c.is_none() {
            diag.temperature_c = smartctl_attr_raw(&smartctl_attrs, 194)
                .or_else(|| smartctl_attr_raw(&smartctl_attrs, 190))
                .map(raw_temp_to_celsius);
        }
        if diag.power_on_hours.is_none() {
            diag.power_on_hours = smartctl_attr_raw_u64(&smartctl_attrs, 9);
        }
        if diag.power_cycle_count.is_none() {
            diag.power_cycle_count = smartctl_attr_raw_u64(&smartctl_attrs, 12);
        }
        if diag.host_writes_total.is_none() {
            diag.host_writes_total = smartctl_attr_raw_u64(&smartctl_attrs, 241);
        }
        if diag.host_reads_total.is_none() {
            diag.host_reads_total = smartctl_attr_raw_u64(&smartctl_attrs, 242);
        }
        if diag.read_errors_total.is_none() {
            diag.read_errors_total = smartctl_attr_raw_u64(&smartctl_attrs, 1)
                .or_else(|| smartctl_attr_raw_u64(&smartctl_attrs, 187));
        }
        if diag.write_errors_total.is_none() {
            diag.write_errors_total = smartctl_attr_raw_u64(&smartctl_attrs, 200)
                .or_else(|| smartctl_attr_raw_u64(&smartctl_attrs, 181));
        }
        if let Some(used) = derive_endurance_used_from_attrs(&smartctl_attrs) {
            if diag.percentage_used.is_none() || diag.percentage_used.unwrap_or(0.0) <= 0.0 {
                diag.percentage_used = Some(used);
            }
        }
    }

    if payload.get("nvme_smart_health_information_log").is_some() {
        diag.smart_supported = true;
        diag.smart_data_source = merge_smart_source(&diag.smart_data_source, "SMARTCTL_NVME");
    }

    let mut rel = match std::mem::take(&mut diag.reliability) {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    merge_smartctl_reliability(&mut rel, payload, diag.smart_attributes.len());
    diag.reliability = Value::Object(rel);
}

fn merge_smartctl_reliability(
    reliability: &mut Map<String, Value>,
    payload: &Value,
    attr_count: usize,
) {
    insert_if_absent(
        reliability,
        "Smartctl.Device",
        get_string_path(payload, &["device", "name"]).map(Value::String),
    );
    insert_if_absent(
        reliability,
        "Smartctl.DeviceType",
        get_string_path(payload, &["device", "type"]).map(Value::String),
    );
    insert_if_absent(
        reliability,
        "Smartctl.Protocol",
        get_string_path(payload, &["device", "protocol"]).map(Value::String),
    );
    insert_if_absent(
        reliability,
        "Smartctl.ExitStatus",
        get_u64_path(payload, &["smartctl", "exit_status"]).map(Value::from),
    );
    insert_if_absent(
        reliability,
        "Smartctl.RotationRate",
        get_u64_path(payload, &["rotation_rate"]).map(Value::from),
    );

    if let Some(capacity) = get_u64_path(payload, &["user_capacity", "bytes"]) {
        insert_if_absent(
            reliability,
            "Smartctl.UserCapacityBytes",
            Some(Value::from(capacity)),
        );
    }

    if payload.get("ata_smart_attributes").is_some() {
        insert_if_absent(
            reliability,
            "Smartctl.AtaAttributeCount",
            Some(Value::from(attr_count as u64)),
        );
    }

    let nvme_fields = [
        ("Nvme.CriticalWarning", "critical_warning"),
        ("Nvme.AvailableSpare", "available_spare"),
        ("Nvme.AvailableSpareThreshold", "available_spare_threshold"),
        ("Nvme.PercentageUsed", "percentage_used"),
        ("Nvme.DataUnitsRead", "data_units_read"),
        ("Nvme.DataUnitsWritten", "data_units_written"),
        ("Nvme.HostReads", "host_reads"),
        ("Nvme.HostWrites", "host_writes"),
        ("Nvme.ControllerBusyTime", "controller_busy_time"),
        ("Nvme.PowerCycles", "power_cycles"),
        ("Nvme.PowerOnHours", "power_on_hours"),
        ("Nvme.UnsafeShutdowns", "unsafe_shutdowns"),
        ("Nvme.MediaErrors", "media_errors"),
        ("Nvme.ErrorLogEntries", "num_err_log_entries"),
    ];

    for (key, field) in nvme_fields {
        let path = ["nvme_smart_health_information_log", field];
        if let Some(v) = get_path(payload, &path).and_then(value_to_json_scalar) {
            insert_if_absent(reliability, key, Some(v));
        }
    }
}

fn parse_smartctl_ata_attributes(payload: &Value) -> Vec<SmartAttribute> {
    let mut attrs = Vec::new();
    let Some(table) =
        get_path(payload, &["ata_smart_attributes", "table"]).and_then(Value::as_array)
    else {
        return attrs;
    };

    for item in table {
        let Some(id) = get_path(item, &["id"]).and_then(value_to_u64) else {
            continue;
        };
        let name = get_string_path(item, &["name"]).unwrap_or_else(|| format!("Attribute {}", id));
        let current = get_path(item, &["value"])
            .and_then(value_to_u64)
            .map(|v| v as u32);
        let worst = get_path(item, &["worst"])
            .and_then(value_to_u64)
            .map(|v| v as u32);
        let threshold = get_path(item, &["thresh"])
            .and_then(value_to_u64)
            .map(|v| v as u32);
        let raw = get_path(item, &["raw"])
            .and_then(|v| get_path(v, &["value"]).or(Some(v)))
            .and_then(value_to_u64);
        let raw_hex = raw
            .map(|v| format!("0x{:X}", v))
            .unwrap_or_else(String::new);

        attrs.push(SmartAttribute {
            id: id as u32,
            name,
            current,
            worst,
            threshold,
            raw,
            raw_hex,
        });
    }
    attrs
}

fn smartctl_attr_raw(attrs: &[SmartAttribute], id: u32) -> Option<f64> {
    attrs
        .iter()
        .find(|a| a.id == id)
        .and_then(|a| a.raw)
        .map(|v| v as f64)
}

fn smartctl_attr_raw_u64(attrs: &[SmartAttribute], id: u32) -> Option<u64> {
    attrs.iter().find(|a| a.id == id).and_then(|a| a.raw)
}

fn smartctl_attr_current(attrs: &[SmartAttribute], id: u32) -> Option<u32> {
    attrs.iter().find(|a| a.id == id).and_then(|a| a.current)
}

fn derive_endurance_used_from_attrs(attrs: &[SmartAttribute]) -> Option<f64> {
    // CDI-like priority:
    // 1) Life-left normalized attributes (231/233/202): used = 100 - current
    // 2) Vendor fallback: RAW 202 directly represents used% for some SSD families.
    let life_left = smartctl_attr_current(attrs, 231)
        .or_else(|| smartctl_attr_current(attrs, 233))
        .or_else(|| smartctl_attr_current(attrs, 202));

    if let Some(v) = life_left {
        if v > 0 && v < 100 {
            return Some((100.0 - v as f64).clamp(0.0, 100.0));
        }
    }

    if let Some(raw_used) = smartctl_attr_raw_u64(attrs, 202) {
        if raw_used > 0 && raw_used <= 100 {
            return Some(raw_used as f64);
        }
    }

    None
}

fn normalize_endurance_percentage(diagnostics: &mut [DiskDiagnostics]) {
    for diag in diagnostics.iter_mut() {
        if is_nvme_diag(diag) {
            continue;
        }

        let estimated = derive_endurance_used_from_attrs(&diag.smart_attributes);

        match (diag.percentage_used, estimated) {
            (Some(v), Some(used)) if v <= 0.0 && used > 0.0 => {
                diag.percentage_used = Some(used);
            }
            (Some(v), None) if v <= 0.0 => {
                // Placeholder zero from some drivers is misleading for SATA/USB.
                diag.percentage_used = None;
            }
            (None, Some(used)) if used > 0.0 => {
                diag.percentage_used = Some(used);
            }
            _ => {}
        }
    }
}

fn raw_temp_to_celsius(raw: f64) -> f64 {
    if raw <= 200.0 {
        raw
    } else {
        (raw as u64 & 0xFF) as f64
    }
}

fn extract_smartctl_temperature(payload: &Value) -> Option<f64> {
    if let Some(temp) = get_f64_path(payload, &["temperature", "current"]) {
        return Some(temp);
    }

    if let Some(mut nvme_temp) = get_f64_path(
        payload,
        &["nvme_smart_health_information_log", "temperature"],
    ) {
        if nvme_temp > 200.0 {
            nvme_temp -= 273.15;
        }
        return Some((nvme_temp * 10.0).round() / 10.0);
    }

    None
}

fn merge_smart_source(existing: &str, add: &str) -> String {
    if existing.is_empty() || existing.eq_ignore_ascii_case("none") {
        return add.to_string();
    }
    if existing.split('+').any(|x| x.eq_ignore_ascii_case(add)) {
        return existing.to_string();
    }
    format!("{existing}+{add}")
}

fn add_note_unique(diag: &mut DiskDiagnostics, note: &str) {
    if !diag.notes.iter().any(|n| n == note) {
        diag.notes.push(note.to_string());
    }
}

fn insert_if_absent(map: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if map.contains_key(key) {
        return;
    }
    if let Some(v) = value {
        if !v.is_null() {
            map.insert(key.to_string(), v);
        }
    }
}

fn get_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for segment in path {
        current = current.get(*segment)?;
    }
    Some(current)
}

fn get_string_path(value: &Value, path: &[&str]) -> Option<String> {
    let v = get_path(value, path)?;
    if let Some(s) = v.as_str() {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

fn get_u64_path(value: &Value, path: &[&str]) -> Option<u64> {
    get_path(value, path).and_then(value_to_u64)
}

fn get_f64_path(value: &Value, path: &[&str]) -> Option<f64> {
    get_path(value, path).and_then(value_to_f64)
}

fn get_bool_path(value: &Value, path: &[&str]) -> Option<bool> {
    get_path(value, path).and_then(Value::as_bool)
}

fn value_to_json_scalar(value: &Value) -> Option<Value> {
    match value {
        Value::Null => None,
        Value::Bool(_) | Value::Number(_) | Value::String(_) => Some(value.clone()),
        Value::Object(_) => {
            if let Some(n) = value_to_u64(value) {
                Some(Value::from(n))
            } else if let Some(f) = value_to_f64(value) {
                Some(Value::from(f))
            } else {
                None
            }
        }
        Value::Array(_) => None,
    }
}

fn value_to_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(n) => n.as_u64().or_else(|| n.as_f64().map(|f| f.max(0.0) as u64)),
        Value::String(s) => parse_u64_string(s),
        Value::Object(map) => map.get("value").and_then(value_to_u64),
        _ => None,
    }
}

fn value_to_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => parse_f64_string(s),
        Value::Object(map) => map.get("value").and_then(value_to_f64),
        _ => None,
    }
}

fn parse_u64_string(s: &str) -> Option<u64> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(n) = trimmed.parse::<u64>() {
        return Some(n);
    }

    let digits: String = trimmed.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse::<u64>().ok()
    }
}

fn parse_f64_string(s: &str) -> Option<f64> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(n) = trimmed.parse::<f64>() {
        return Some(n);
    }

    let normalized: String = trimmed
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    if normalized.is_empty() {
        None
    } else {
        normalized.parse::<f64>().ok()
    }
}

fn is_masked_serial(serial: &str) -> bool {
    let normalized: String = serial
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();

    if normalized.len() < 4 {
        return true;
    }
    if normalized.chars().all(|c| c == '0' || c == 'D') {
        return true;
    }
    if normalized.chars().all(|c| c == '0') {
        return true;
    }
    if normalized.chars().all(|c| c == 'F') {
        return true;
    }
    false
}

fn parse_disk(v: &serde_json::Value) -> DiskInfo {
    let number = v["Number"].as_u64().unwrap_or(0);
    let name = v["FriendlyName"].as_str().unwrap_or("Unknown").to_string();
    let size = v["Size"].as_u64().unwrap_or(0);
    let bus_type = v["BusType"].as_u64().unwrap_or(0);
    let media_type_raw = v["MediaType"].as_str().unwrap_or("").to_string();
    let is_system =
        v["IsSystem"].as_bool().unwrap_or(false) || v["IsBoot"].as_bool().unwrap_or(false);
    let bus_name = v["BusTypeName"]
        .as_str()
        .or_else(|| v["BusType"].as_str())
        .unwrap_or("Other")
        .to_string();
    // Removable buses: USB/SD/MMC
    let removable = if bus_type == 7 || bus_type == 12 || bus_type == 13 {
        true
    } else {
        let bus_up = bus_name.to_uppercase();
        bus_up.contains("USB") || bus_up == "SD" || bus_up == "MMC"
    };
    let device = format!("PhysicalDrive{}", number);

    let drive_type = if removable {
        "Removable".to_string()
    } else {
        format!("Fixed ({})", bus_name)
    };

    let volume = v["VolumeLetter"].as_str().unwrap_or("").to_string();

    DiskInfo {
        id: format!("disk{}", number),
        name,
        size,
        removable,
        device,
        drive_type,
        media_type: normalize_media_type(&media_type_raw),
        index: number.to_string(),
        volume,
        is_system,
    }
}

fn normalize_media_type(raw: &str) -> String {
    let up = raw.to_uppercase();
    if up.contains("SSD") || up.contains("NVME") || up == "4" {
        "SSD".to_string()
    } else if up.contains("HDD") || up.contains("ROTATIONAL") || up == "3" {
        "HDD".to_string()
    } else if up.contains("UNKNOWN") || up.contains("UNSPECIFIED") || up.trim().is_empty() {
        "Unknown".to_string()
    } else {
        raw.trim().to_string()
    }
}

/// Get disk info on Windows
pub async fn get_disk_info(disk_id: &str) -> Result<DiskInfo> {
    let disks = list_disks().await?;
    disks
        .into_iter()
        .find(|d| d.id == disk_id)
        .ok_or_else(|| AppError::DeviceNotFound(disk_id.to_string()))
}

/// Start USB monitoring on Windows
pub async fn start_usb_monitoring(_app_handle: tauri::AppHandle) -> Result<String> {
    Ok("monitor-windows".to_string())
}

/// Stop USB monitoring on Windows
pub async fn stop_usb_monitoring(_monitor_id: &str) -> Result<()> {
    Ok(())
}
