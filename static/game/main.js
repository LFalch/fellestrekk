function randomInt(min, max) {
    if (!max) {
        max = min;
        min = 0;
    }

    return Math.floor(Math.random() * (max - min) + min);
}

let app = new PIXI.Application({ width: 800, height: 600 });

//Add the canvas that Pixi automatically created for you to the HTML document
document.body.appendChild(app.view);

PIXI.Loader.shared.add("cards.png").load(setup);

let CARD;
let socket;

function setup() {
    socket = new WebSocket(`ws://${document.location.hostname}:2794`, "fellestrekk");
    socket.onmessage = onMessage;
    socket.onclose = onClose;
    socket.onopen = function() {
        socket.send("HOST");
    }

    const cards_tex = PIXI.Loader.shared.resources["cards.png"].texture.baseTexture
    let suits = [];
    for (let y = 0; y < cards_tex.height; y += 96) {
        let cards = [];
        for (let x = 0; x < cards_tex.width; x += 72) {
            const card_tex = new PIXI.Texture(cards_tex, new PIXI.Rectangle(x, y, 72, 95));
            cards.push(card_tex);
        }
        suits.push(cards);
    }

    function mkSprte(tex) {
        return new PIXI.Sprite(tex);
    }

    CARD = suits;
    CARD.__proto__.card = function(c) {
        if (c >= 52) {
            return this.joker(Math.floor((c-52)/2));
        } else if (c < 0) {
            return this.backCard(-c-1);
        }
        const i = c % 13;
        const j = Math.floor(c / 13);

        return mkSprte(this[j][i]);
    }
    CARD.__proto__.joker = function(colour = 0) {
        return mkSprte(this[2+colour][13]);
    }
    CARD.__proto__.backCard = function(colour = 0) {
        return mkSprte(this[colour][13]);
    }

    app.ticker.add(mkGmLoop(consistentLogic));
}

let deck = [];
let cards = [];

app.view.addEventListener('click', () => socket.send("HIT"));
window.addEventListener('keydown', (event) => onKeyDown(event), false);

function onKeyDown(event) {
    console.log(event);
    if (event.code == 'KeyS') {
        socket.send("STAND");
    }
}

function mkGmLoop(logic) {
    let time = 0;

    return function gameLoop(delta) {
        time += delta;

        for (let i = 0; time >= 1 && i < 5; i++) {
            time -= 1;
            logic();
        }
    }
}

function consistentLogic() {

}

const increment = 12;
const hole_card_x = 15;
const hole_card_y = 400;
let hold_card_n = 0;

/**
 * @param {MessageEvent<string>} [event] - event.
 */
function onMessage(event) {
    if (event.data.startsWith('PING')) {
        socket.send('PONG');
    } else if (event.data.startsWith('PONG')) {
    } else if (event.data.startsWith('LOSE')) {
        cards.forEach(spr => app.stage.removeChild(spr));
        cards = [];
    } else if (event.data.startsWith('DECKSIZE ')) {
        const args = event.data.substr(9).split(' ');

        const decksize = Number(args[0]) / 10;
        deck.forEach(spr => app.stage.removeChild(spr));
        deck = [];
        for (let i = 0; i < decksize; i++) {
            const card = app.stage.addChild(CARD.backCard());
            card.position = {y: 10, x: 20 + i * 2};
            deck.push(card);
        }
    } else if (event.data.startsWith('HOLECARDS ')) {
        const args = event.data.substr(10).split(' ');
        hold_card_n = args.length;

        let x = hole_card_x;
        const y = hole_card_y;
        for (const arg of args) {
            const c = parseCard(arg);

            const card = app.stage.addChild(CARD.card(c));
            card.position = {y, x};
            cards.push(card);
            x += increment;
        }
    } else if (event.data.startsWith('DEALERHOLE ')) {
        const args = event.data.substr(11).split(' ');

        let x = 15;
        const y = 200;

        let card = app.stage.addChild(CARD.backCard());
        card.position = { y, x };
        cards.push(card);
        x += increment;

        const c = parseCard(args[0]);
        card = app.stage.addChild(CARD.card(c));
        card.position = { y, x };
        cards.push(card);
    } else if (event.data.startsWith('DRAWN ')) {
        const args = event.data.substr(6).split(' ');

        const c = parseCard(args[0]);

        const card = app.stage.addChild(CARD.card(c));
        card.position = { y: hole_card_y, x: hole_card_x + increment * hold_card_n };
        hold_card_n += 1;
        cards.push(card);
    } else {
        console.log(`unknown packet ${event.data}`);
    }
}
function onClose(event) {

}

function parseCard(s) {
    let c;
    switch (s[0]) {
        case '♣':
            c = 0;
            break;
        case '♥':
            c = 13
            break;
        case '♠':
            c = 26;
            break;
        case '♦':
            c = 39;
            break;
    }
    switch (s[1]) {
        case 'K':
            c += 12;
            break;
        case 'Q':
            c += 11;
            break;
        case 'J':
            c += 10;
            break;
        case 'A':
            break;
        default:
            c += Number(s[1]) - 1;
            break;
    }
    return c;
}
