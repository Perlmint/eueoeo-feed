FROM node as builder

RUN mkdir /ws
WORKDIR /ws
COPY package.json tsconfig.json /ws/
RUN npm i --no-package-lock
COPY src /ws/src
RUN npm run build

FROM node:alpine

RUN mkdir /app
WORKDIR /app
COPY package.json /app/
RUN npm i --omit dev
COPY --from=builder /ws/dist/ /app/dist/

EXPOSE 3000
CMD [ "node", "/app/dist/index.js" ]
