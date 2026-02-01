#!/bin/bash

# 颜色定义
GREEN='\033[0;32m'
NC='\033[0m' # No Color

echo -e "${GREEN}Starting GPU Monitor Development Environment...${NC}"

# 0. 清理环境
echo "Cleaning up port 5173..."
fuser -k 5173/tcp >/dev/null 2>&1

# 生成临时图标（如果不存在）
if [ ! -f "icons/icon.png" ]; then
    echo "Generating placeholder icons..."
    mkdir -p icons
    # 创建一个简单的 1x1 像素 PNG 文件
    echo -n -e '\x89\x50\x4e\x47\x0d\x0a\x1a\x0a\x00\x00\x00\x0d\x49\x48\x44\x52\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89\x00\x00\x00\x0a\x49\x44\x41\x54\x78\x9c\x63\x00\x01\x00\x00\x05\x00\x01\x0d\x0a\x2d\xb4\x00\x00\x00\x00\x49\x45\x4e\x44\xae\x42\x60\x82' > icons/icon.png
    cp icons/icon.png icons/32x32.png
    cp icons/icon.png icons/128x128.png
    cp icons/icon.png icons/128x128@2x.png
    cp icons/icon.png icons/icon.icns
    cp icons/icon.png icons/icon.ico
fi

# 1. 进入前端目录并安装依赖（如果需要）
cd src-web
if [ ! -d "node_modules" ]; then
    echo -e "${GREEN}Installing frontend dependencies...${NC}"
    npm install
fi

# 2. 后台启动前端服务
echo -e "${GREEN}Starting frontend server...${NC}"
npm run dev &
FRONTEND_PID=$!

# 等待端口 5173 准备就绪
echo "Waiting for frontend to be ready on port 5173..."
while ! nc -z localhost 5173; do
  sleep 0.5
done
echo -e "${GREEN}Frontend is ready!${NC}"

# 3. 返回 GUI 根目录并启动 Tauri
cd ..
echo -e "${GREEN}Starting Tauri backend...${NC}"
cargo tauri dev

# 4. 退出时清理前端进程
kill $FRONTEND_PID
