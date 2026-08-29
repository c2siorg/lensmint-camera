FROM node:18-alpine

WORKDIR /app

COPY package.json package-lock.json ./

RUN npm install --production

COPY . .

RUN mkdir -p ./database

EXPOSE 5000

CMD ["node", "server.js"]
