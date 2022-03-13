DOCKER_BUILDKIT=0 docker build . --file Dockerfile.bot.builder --tag ghcr.io/axmouth/disco-music-bot-builder
DOCKER_BUILDKIT=0 docker build . --file Dockerfile.bot.final --tag ghcr.io/axmouth/disco-music-bot
docker-compose build
docker-compose up -d --remove-orphans