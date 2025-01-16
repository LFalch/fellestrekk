// TODO: update to new API
// TODO: support multiplayer

function randomInt(min, max) {
    if (!max) {
        max = min;
        min = 0;
    }

    return Math.floor(Math.random() * (max - min) + min);
}

let app = new PIXI.Application({ width: 800, height: 600 });

document.getElementById('game').appendChild(app.view);

PIXI.Loader.shared.add("static/cards.png").load(setup);

let texes = {};
let strings = {};
let onReloadStringsCbs = [];
let animationQueue = [];

const onReloadStrings = function () {
    for (const cb of onReloadStringsCbs) {
        cb();
    }
}
{
    const httpReq = new XMLHttpRequest();
    const lang = document.documentElement.lang;
    httpReq.open("GET", `/strings/${lang}.json`, true);
    httpReq.onreadystatechange = function () {
        if (httpReq.readyState == 4 && httpReq.status == 200) {
            strings = JSON.parse(httpReq.responseText);
        }

        onReloadStrings();
    }
    httpReq.send(null);
}

function texture(name) {
    if (!texes[name]) {
        texes[name] = PIXI.Loader.shared.resources[`static/${name}.png`].texture;
    }

    return texes[name];
}

let CARD;
let socket;
let statusText;
let dealerHandText;
let playerHandText;
let balanceText;
let balance = 1000;

function setup() {
    app.stage.interactive = true;
    app.stage.sortableChildren = true;

    let protocol = "ws:";
    if (window.location.protocol === "https:") {
        protocol = "wss:";
    }
    socket = new WebSocket(`${protocol}//${document.location.hostname}:${document.location.port}/ws`);
    socket.onmessage = onMessage;
    socket.onclose = onClose;
    socket.onopen = function () {
        const get_code = new URLSearchParams(document.location.search).get('code');

        if (get_code == null) {
            socket.send(`HOST BLACKJACK`)
        } else {
            code = get_code;
            socket.send(`JOIN ${code}`);
        }
    }

    document.getElementById('formChat').onsubmit = function (event) {
        target = event.target;
        const msg = event.target.firstElementChild.value;
        event.target.firstElementChild.value = '';

        socket.send(`CHAT ${msg}`);

        event.preventDefault();
    }

    const cards_tex = PIXI.Loader.shared.resources["static/cards.png"].texture.baseTexture
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

    playerHandText = new PIXI.Text('Value: ', {fontFamily:'Arial',fontSize:20, fill: 0xffffff, align: 'left'});
    dealerHandText = new PIXI.Text('Value: ', {fontFamily:'Arial',fontSize:20, fill: 0xffffff, align: 'left'});
    playerHandText.position = {x: 4, y: 495};
    dealerHandText.position = {x: 4, y: 295};
    app.stage.addChild(playerHandText);
    app.stage.addChild(dealerHandText);

    balanceText = new PIXI.Text(`Balance: ¤${balance}`, { fontFamily: 'Arial', fontSize: 20, fill: 0xffffff, align: 'left' });
    balanceText.position = { x: 570, y: 0};
    app.stage.addChild(balanceText);

    statusText = new PIXI.Text('[H]it [S]tand', {fontFamily:'Arial',fontSize:20, fill: 0xffffff, align: 'left'});
    statusText.position = {x: 4, y: 572};
    app.stage.addChild(statusText);
}

/**
 * @param {string} [sender] - Sender name.
 * @param {string} msg - Message body.
 * @param {string} [msg_class] - CSS class for message body
 */
function msgBox(sender, msg, msg_class) {
    if (!msg) {
        msg = sender;
        sender = null;
    }

    const sender_txt = sender ? document.createTextNode(`${sender}: `) : null;
    const msg_txt = document.createTextNode(msg);

    let p = document.createElement("p");

    if (sender_txt) {
        let b = document.createElement("b");
        b.appendChild(sender_txt);

        p.appendChild(b);
    }

    let span = document.createElement("span");
    span.appendChild(msg_txt);
    if (msg_class) {
        span.classList.add(msg_class);
    }

    p.appendChild(span);

    const bm = document.getElementById('boxMessages');

    const scroll = bm.scrollHeight - bm.scrollTop == bm.clientHeight;

    bm.appendChild(p);
    if (scroll) {
        bm.scrollBy(0, p.clientHeight);
    }

    return p;
}

let deck = [];
let dealerhand = []
let playerhand = [];
let forceFinishNextAnimation = false;

function onKeyDown(event) {
    switch (event.code) {
        case 'KeyS':
            socket.send("STAND");
            break;
        case 'KeyH':
            socket.send("HIT");
            break;
        case 'KeyD':
            socket.send("DOUBLEDOWN");
            break;
        case 'KeyU':
            socket.send("SURRENDER");
            break;
        case 'KeyP':
            socket.send("SPLIT");
            break;
        case 'KeyN':
            socket.send('BET 100');
            socket.send("START");
            break;
        default:
            forceFinishNextAnimation = true;
            break;
    }
    if (animationQueue.length > 1) forceFinishNextAnimation = true;
}
app.view.setAttribute('tabindex', 1);
app.view.addEventListener('keydown', (event) => onKeyDown(event), false);

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

function dummyAnimation(fireOnce) {
    return {progress: (delta) => {
        fireOnce(delta);
        return true;
    }}
}
function CardAnimation(sprite, end_x, end_y, total_time, maybe_flip = null) {
    const dist_x = end_x - sprite.x;
    const dist_y = end_y - sprite.y;
    if (typeof total_time === 'object') {
        if (total_time.speed) {
            total_time = Math.hypot(dist_x, dist_y) / total_time.speed;
        }
    }

    const dx = dist_x / total_time;
    const dy = dist_y / total_time;
    const ow = sprite.width;
    const dw = ow / (total_time / 2);

    let time = 0;
    let spread_out = false;
    this.progress = (delta, force_finish = false) => {
        if (!app.stage.children.includes(sprite)) return true;

        time += delta;
        const animationIsDone = time > total_time;
        if (animationIsDone || force_finish) {
            sprite.x = end_x;
            sprite.y = end_y;
            sprite.width = ow;
            if (maybe_flip !== null)
                sprite.texture = maybe_flip;

            return animationIsDone;
        }

        if (maybe_flip !== null) {
            if (time > total_time / 2) {
                sprite.texture = maybe_flip;
                maybe_flip = null;
                spread_out = true;
            } else {
                const ddw = dw * delta;
                sprite.width -= ddw;
                sprite.x += ddw/2;
            }
        } else if (spread_out) {
            const ddw = dw * delta;
            sprite.width += ddw;
            sprite.x -= ddw/2;
        }

        sprite.x += dx * delta;
        sprite.y += dy * delta;

        return false;
    };
    return this;
}
function queueAnimation(animation) {
    animationQueue.push(animation);
}
function drawCard(targetX, targetY, c = null) {
    const card = app.stage.addChild(CARD.backCard());
    card.position = { x: DECK_X + (deck.length-1) * 2, y: DECK_Y};
    let flipSide = null;
    if (c) flipSide = CARD.card(c).texture;
    queueAnimation(new CardAnimation(card, targetX, targetY, {speed:800}, flipSide));
    return card;
}

function consistentLogic() {
    const force = forceFinishNextAnimation;
    let forced = false;
    forceFinishNextAnimation = false;
    const delta = 1 / 60;
    while (animationQueue.length > 0 && (animationQueue[0].progress(delta, force) || (force && (forced = true)))) {
        animationQueue.shift();
        if (forced) break;
    }
}

const increment = 12;
const hole_card_x = 15;
const hole_card_y = 400;

function updateBalance(difference) {
    balance += difference;
    balanceText.text = `Balance: $${balance}\nAuto-bet: $100`;
}

const DECK_X = 20;
const DECK_Y = 10;

function onMessage(event) {
    console.log(`got ${event.data}`);
    if (event.data.startsWith('PING')) {
        socket.send('PONG');
    } else if (event.data.startsWith('HOST_OK')) {
        socket.send('BET 100');
        socket.send("START");
    } else if (event.data.startsWith('LOSE')) {
        statusText.text = 'You lost! :( ' + statusText.text;
    } else if (event.data.startsWith('WIN')) {
        statusText.text = 'You won!!!  ' + statusText.text;
    } else if (event.data.startsWith('DRAW')) {
        statusText.text = 'You tied! You get the bet back. ' + statusText.text;
    } else if (event.data.startsWith('TAKEMONEY ')) {
        const args = event.data.substr(10).split(' ');
        const money = Number(args[0]);
        updateBalance(-money);
    } else if (event.data.startsWith('SENDMONEY ')) {
        const args = event.data.substr(10).split(' ');
        const money = Number(args[0]);
        updateBalance(money);
    } else if (event.data.startsWith('DECKSIZE ')) {
        const args = event.data.substr(9).split(' ');

        const decksize = Number(args[0]) / 5;
        while (deck.length < decksize) {
            const card = app.stage.addChild(CARD.backCard());
            card.position = {y: DECK_Y, x: DECK_X + deck.length * 2};
            deck.push(card);
        }
        while (deck.length > decksize) {
            const card = deck.pop();
            app.stage.removeChild(card);
        }
    } else if (event.data.startsWith('START')) {
        dealerhand.forEach(spr => app.stage.removeChild(spr));
        dealerhand = [];
        playerhand.forEach(spr => app.stage.removeChild(spr));
        playerhand = [];
    } else if (event.data.startsWith('VALUEUPDATE ')) {
        const args = event.data.substr(12).split(' ');
        const soft = args[args.length-1] == 'soft';
        const value = Number(args[args.length-(soft?2:1)]);
        let text;
        if (args.length > (soft?2:1)) {
            text = playerHandText;
        } else {
            text = dealerHandText;
        }
        text.text = `Value: ${value}`;
        if (soft) text.text += ` or ${value - 10}`;
    } else if (event.data.startsWith('STATUS ')) {
        const args = event.data.substr(7).split(' ');
        statusText.text = ' ';
        for (const arg of args) {
            switch (arg) {
                case 'H':
                    statusText.text += " [H]it";
                    break;
                case 'S':
                    statusText.text += " [S]tand";
                    break;
                case 'D':
                    statusText.text += " [D]ouble down";
                    break;
                case 'U':
                    statusText.text += " S[U]rrender";
                    break;
                case 'P':
                    statusText.text += " S[P]lit";
                    break;
                case 'N':
                    statusText.text += " [N]ew game";
                    break;
                default:
                    console.log(`unknown capabiltity ${arg}`);
                    break;
            }
        }
        
    } else if (event.data.startsWith('REVEALDOWNS ')) {
        const args = event.data.substr(12).split(' ');
        const c = parseCard(args[0]);
        const card = dealerhand[0];

        queueAnimation(dummyAnimation(() => card.zIndex += 1));
        queueAnimation(new CardAnimation(card, 15, 130, 0.4, CARD.card(c).texture));
        queueAnimation(dummyAnimation(() => {
            card.zIndex -= 2;
            queueAnimation(new CardAnimation(card, 15, 200, 0.35));
        }));
    } else if (event.data.startsWith('DOWNCARD ')) {
        const args = event.data.substr(9).split(' ');
        const c = parseCard(args[0]);

        const card = drawCard(hole_card_x, hole_card_y, c);
        playerhand.push(card);

        if (dealerhand.length == 0) {
            const card = drawCard(15, 200);
            dealerhand.push(card);
        }
    } else if (event.data.startsWith('DEALERDRAW ')) {
        const args = event.data.substr(11).split(' ');

        const y = 200;

        if (dealerhand.length == 0 ) {
            const card = drawCard(15 + dealerhand.length * increment, y);
            dealerhand.push(card);
        }
        const c = parseCard(args[0]);
        queueAnimation(dummyAnimation(() => dealerhand.push(drawCard(15 + dealerhand.length * increment, y, c))));
    } else if (event.data.startsWith('PLAYERDRAW ')) {
        const args = event.data.substr(11).split(' ');

        const c = parseCard(args[1]);

        const card = drawCard(hole_card_x + increment * playerhand.length, hole_card_y, c);
        playerhand.push(card);
    } else if (event.data.startsWith('CHAT_MSG ')) {
        const body = event.data.substr('CHAT_MSG '.length);
        const sender = body.split(' ')[0];
        const sender_name = `${strings.player} ${Number(sender)+1}`;
        const msg = body.substr(sender.length + 1);

        msgBox(sender_name, msg);
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
    switch (s.substr(1)) {
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
            c += Number(s.substr(1)) - 1;
            break;
    }
    return c;
}

function sniff() {
    if (socket.actualSend) {
        socket.send = socket.actualSend;
        delete socket.actualSend;
        return
    }
    socket.actualSend = socket.send;
    socket.send = function(packet) {
        socket.actualSend(packet);
        console.log(`sent ${packet}`);
    }
}
