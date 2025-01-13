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
let statusText;
let dealerHandText;
let playerHandText;
let balanceText;
let balance = 1000;

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

let deck = [];
let cards = [];

window.addEventListener('keydown', (event) => onKeyDown(event), false);

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
let dealer_card_n = 0;
let dealer_hole_card;

function updateBalance(difference) {
    balance += difference;
    balanceText.text = `Balance: $${balance}\nAuto-bet: $100`;
}

/**
 * @param {MessageEvent<string>} [event] - event.
 */
function onMessage(event) {
    console.log(`packet ${event.data}`);
    if (event.data.startsWith('PING')) {
        socket.send('PONG');
    } else if (event.data.startsWith('HOST_OK')) {
        socket.send('BET 100');
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
            card.position = {y: 10, x: 20 + deck.length * 2};
            deck.push(card);
        }
        while (deck.length > decksize) {
            const card = deck.pop();
            app.stage.removeChild(card);
        }
    } else if (event.data.startsWith('START')) {
        hold_card_n = 0;
        dealer_card_n = 0;
        cards.forEach(spr => app.stage.removeChild(spr));
        cards = [];
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

        dealer_hole_card.texture = CARD.card(c).texture;
    } else if (event.data.startsWith('DOWNCARD ')) {
        const args = event.data.substr(9).split(' ');
        const c = parseCard(args[0]);

        const card = app.stage.addChild(CARD.card(c));
        card.position = { y: hole_card_y, x: hole_card_x };
        cards.push(card);
        hold_card_n = 1;
    } else if (event.data.startsWith('DEALERDRAW ')) {
        const args = event.data.substr(11).split(' ');

        let x = 15 + dealer_card_n * increment;
        const y = 200;

        if (dealer_card_n == 0 ) {
            dealer_hole_card = app.stage.addChild(CARD.backCard());
            dealer_hole_card.position = { y, x };
            cards.push(dealer_hole_card);
            x += increment;
            dealer_card_n += 1;
        }
        const c = parseCard(args[0]);
        const card = app.stage.addChild(CARD.card(c));
        card.position = { y, x };
        cards.push(card);

        dealer_card_n += 1;
    } else if (event.data.startsWith('PLAYERDRAW ')) {
        const args = event.data.substr(11).split(' ');

        const c = parseCard(args[1]);

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
