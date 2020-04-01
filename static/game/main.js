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

function setup() {
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
    randomCard();

    app.ticker.add(mkGmLoop(consistentLogic));
}

function randomCard(i = randomInt(56)) {
    console.log(i);
    app.stage.removeChildren();
    app.stage.addChild(CARD.card(i));
}

app.view.addEventListener('click', () => randomCard());

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

let count = 0;

function consistentLogic() {
    count++;
    if (count >= 60) {
        count %= 60;
        randomCard();
    }
}