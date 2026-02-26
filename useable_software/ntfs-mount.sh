#!/bin/bash
# 修复macOS NTFS磁盘只读问题，依赖 macfuse + ntfs-3g
# 适配不同macOS系统的mount输出格式

run_as_root() {
    if [ "$(id -u)" -eq 0 ]; then
        # 已经在root上下文时直接执行，避免再次经过sudo导致环境/目录异常
        "$@"
    else
        sudo "$@"
    fi
}

wait_until_unmounted() {
    dev_path="$1"
    mount_point="$2"
    retry=0
    while [ $retry -lt 15 ]; do
        if ! mount | grep -qE "^${dev_path}[[:space:]]|[[:space:]]on[[:space:]]${mount_point}[[:space:]]"; then
            return 0
        fi
        sleep 0.2
        retry=$((retry + 1))
    done
    return 1
}

is_unsafe_state_text() {
    echo "$1" | grep -qiE 'unsafe state|hibernation|fast restarting'
}

run_ntfs_mount() {
    mount_dev_path="$1"
    mount_point="$2"
    volume_name="$3"
    mode="$4"
    err_file="$5"

    case "$mode" in
        fast)
            run_as_root "$(command -v ntfs-3g)" "$mount_dev_path" "$mount_point" -o rw -o big_writes -o noatime -o noappledouble -o local -o volname="$volume_name" -o nonempty 2>"$err_file"
            ;;
        conservative)
            run_as_root "$(command -v ntfs-3g)" "$mount_dev_path" "$mount_point" -o rw -o big_writes -o noatime -o nonempty 2>"$err_file"
            ;;
        force_remove_hiberfile)
            run_as_root "$(command -v ntfs-3g)" "$mount_dev_path" "$mount_point" -o rw -o big_writes -o noatime -o noappledouble -o remove_hiberfile -o nonempty 2>"$err_file"
            ;;
        *)
            echo "未知挂载模式: $mode" >"$err_file"
            return 9
            ;;
    esac
}

# 检查ntfs-3g是否安装
if ! command -v ntfs-3g &> /dev/null; then
    echo "错误：未找到ntfs-3g，请先执行 brew install macfuse ntfs-3g 安装依赖！"
    exit 1
fi

force_mode=0
case "${WTGA_NTFS_FORCE:-0}" in
    1|true|TRUE|yes|YES|on|ON)
        force_mode=1
        ;;
esac

echo "开始加载可读写的NTFS磁盘..."
# 找到所有已挂载的NTFS设备（兼容不同格式）
newDev=$(mount | awk 'BEGIN{IGNORECASE=1} /ntfs/ {print $1}' | sed 's#^/dev/##' | sed '/^$/d' | sort -u)

# 检查是否有NTFS磁盘
if [ -z "$newDev" ]; then
    echo "未检测到已挂载的NTFS磁盘！"
    exit 0
fi

# 遍历处理每个NTFS设备
failed_count=0
for i in $newDev; do
    # 初始化变量
    disk_name=""
    volume_name=""

    # 适配不同的设备路径格式
    if [[ $i == ntfs://* ]]; then
        # 格式1：ntfs://disk4s1/inbox（原始脚本预期格式）
        onceCutVal=${i%/*}
        disk_name=${onceCutVal#*//}
        volume_name=${i##*/}
    else
        # 格式2：直接是 disk4s3 或 /dev/disk4s3（你的系统格式）
        # 提取纯磁盘名（去掉/dev/前缀）
        disk_name=${i##*/}
        # 卷名默认用磁盘名
        volume_name=$disk_name
    fi

    # 验证磁盘名是否有效
    if [ -z "$disk_name" ]; then
        echo "⚠️  无法解析设备路径：$i，跳过处理"
        continue
    fi

    # 拼接正确的设备路径
    dev_path="/dev/$disk_name"
    raw_dev_path="/dev/r$disk_name"
    mount_dev_path="$dev_path"
    if [ -e "$raw_dev_path" ]; then
        mount_dev_path="$raw_dev_path"
    fi
    # 优先使用当前挂载点，拿不到时回退默认路径
    current_mount=$(mount | awk -v dev="$dev_path" '$1==dev {print $3; exit}')
    if [ -n "$current_mount" ]; then
        mount_point="$current_mount"
    else
        mount_point="/Volumes/$volume_name"
    fi

    echo "正在处理设备: $disk_name (挂载点: $mount_point)"

    # 卸载原有只读挂载（忽略错误，优先走diskutil）
    run_as_root diskutil unmount force "$dev_path" >/dev/null 2>&1 || true
    run_as_root umount "$mount_point" >/dev/null 2>&1 || true
    wait_until_unmounted "$dev_path" "$mount_point" || true

    # 创建挂载点（确保目录存在）
    run_as_root mkdir -p "$mount_point"

    mount_err_file="/tmp/wtga-ntfs-mount-${disk_name}.err"
    : >"$mount_err_file"

    # 重新挂载为可读写（使用正确的设备路径）
    run_ntfs_mount "$mount_dev_path" "$mount_point" "$volume_name" "fast" "$mount_err_file"
    mount_rc=$?
    mount_err="$(cat "$mount_err_file" 2>/dev/null)"

    # 第一种参数失败后，退回更保守的参数再试一次
    if [ $mount_rc -ne 0 ]; then
        sleep 1
        run_ntfs_mount "$mount_dev_path" "$mount_point" "$volume_name" "conservative" "$mount_err_file"
        mount_rc=$?
        mount_err="$(cat "$mount_err_file" 2>/dev/null)"
    fi

    # 强制模式：删除 Windows 休眠文件后再尝试挂载（适用于 WTG 场景）
    if [ $mount_rc -ne 0 ] && [ "$force_mode" -eq 1 ]; then
        echo "⚠️  启用强制模式：将尝试 remove_hiberfile（可能丢失 Windows 未保存会话）"
        if is_unsafe_state_text "$mount_err"; then
            if command -v ntfsfix >/dev/null 2>&1; then
                echo "⚠️  检测到 NTFS 不安全状态，先尝试 ntfsfix -d 清理脏位..."
                run_as_root ntfsfix -d "$dev_path" >/dev/null 2>&1 || true
                run_as_root diskutil unmount force "$dev_path" >/dev/null 2>&1 || true
                run_as_root mkdir -p "$mount_point"
            fi
        fi
        sleep 1
        run_ntfs_mount "$mount_dev_path" "$mount_point" "$volume_name" "force_remove_hiberfile" "$mount_err_file"
        mount_rc=$?
        mount_err="$(cat "$mount_err_file" 2>/dev/null)"
    fi

    if [ $mount_rc -eq 0 ]; then
        echo "✅ 设备 $disk_name 已挂载为可读写模式！"
    else
        echo "❌ 设备 $disk_name 挂载失败，请检查："
        echo "   1. 磁盘是否已解锁/未被占用"
        echo "   2. 是否有其他NTFS工具（如Paragon NTFS）正在运行"
        echo "   3. 尝试手动执行：sudo ntfs-3g $mount_dev_path $mount_point -o rw"
        if [ -n "$mount_err" ]; then
            echo "   4. 详细错误：$mount_err"
        fi
        failed_count=$((failed_count + 1))
    fi
    rm -f "$mount_err_file" >/dev/null 2>&1 || true
    echo '-------------------------'
done

echo "所有NTFS设备处理完成！"
if [ "$failed_count" -gt 0 ]; then
    echo "失败设备数量：$failed_count"
    exit 2
fi
exit 0
