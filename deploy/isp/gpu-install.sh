#!/bin/bash
# NVIDIA driver (production branch) and the container toolkit, for the mixer's
# hardware codecs. Logs to /root/gpu-install.log. A reboot follows separately.
set -x
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y nvidia-driver-580-server nvidia-utils-580-server libnvidia-encode-580-server libnvidia-decode-580-server
curl -fsSL https://nvidia.github.io/libnvidia-container/gpgkey | gpg --dearmor --yes -o /usr/share/keyrings/nvidia-container-toolkit-keyring.gpg
curl -fsSL https://nvidia.github.io/libnvidia-container/stable/deb/nvidia-container-toolkit.list | sed 's#deb https://#deb [signed-by=/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg] https://#g' > /etc/apt/sources.list.d/nvidia-container-toolkit.list
apt-get update -qq
apt-get install -y nvidia-container-toolkit
nvidia-ctk runtime configure --runtime=docker
echo GPU-INSTALL-DONE
