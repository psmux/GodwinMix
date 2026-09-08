FROM node:20-bookworm-slim
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
COPY . .
# vite writes the client to client/dist (root client/, outDir dist); the server
# in production serves dist/public next to dist/index.js.
RUN npm run build && mkdir -p dist/public && cp -r client/dist/. dist/public/
EXPOSE 5001
CMD ["npm", "start"]
