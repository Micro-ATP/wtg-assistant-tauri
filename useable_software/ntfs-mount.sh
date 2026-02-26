#!/bin/bash
# 修复macOS NTFS磁盘只读问题，依赖 macfuse + ntfs-3g
# 适配不同macOS系统的mount输出格式

# 检查ntfs-3g是否安装
if ! command -v ntfs-3g &> /dev/null; then
    echo "错误：未找到ntfs-3g，请先执行 brew install macfuse ntfs-3g 安装依赖！"
    exit 1
fi

echo "开始加载可读写的NTFS磁盘..."
# 找到所有已挂载的NTFS设备（兼容不同格式）
newDev=$(mount | grep -i ntfs | awk -F ' ' '{print $1}')

# 检查是否有NTFS磁盘
if [ -z "$newDev" ]; then
    echo "未检测到已挂载的NTFS磁盘！"
    exit 0
fi

# 遍历处理每个NTFS设备
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
    # 拼接挂载点路径
    mount_point="/Volumes/$volume_name"

    echo "正在处理设备: $disk_name (挂载点: $mount_point)"

    # 卸载原有只读挂载（忽略错误）
    sudo umount "$i" 2>/dev/null

    # 创建挂载点（确保目录存在）
    sudo mkdir -p "$mount_point"

    # 重新挂载为可读写（使用正确的设备路径）
    sudo $(which ntfs-3g) "$dev_path" "$mount_point" -o local -o allow_other -o auto_xattr -o volname="$volume_name"

    if [ $? -eq 0 ]; then
        echo "✅ 设备 $disk_name 已挂载为可读写模式！"
    else
        echo "❌ 设备 $disk_name 挂载失败，请检查："
        echo "   1. 磁盘是否已解锁/未被占用"
        echo "   2. 是否有其他NTFS工具（如Paragon NTFS）正在运行"
        echo "   3. 尝试手动执行：sudo ntfs-3g $dev_path $mount_point -o rw"
    fi
    echo '-------------------------'
done

echo "所有NTFS设备处理完成！"