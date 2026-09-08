FROM node:20-bookworm-slim
WORKDIR /sim
COPY package.json package-lock.json ./
RUN npm ci
COPY . .
CMD ["npm", "run", "play", "--", "--help"]
