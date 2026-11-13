// The mini-app's Qt Quick Test runner with Logos Core behind it (e2e/seqnet: the user's round trip on a running
// LEZ sequencer). Two objects are given to the QML engine:
//
//  - `logos`: the host's bridge as a ui module sees it inside Logos (Main.qml's module transport calls
//    logos.callModule("ferrata_bridge_module", method, [json]) and reads the reply). Here every call goes to a
//    headless Logos Core: `logosctl call ferrata_bridge_module <method> @<request file>` against the daemon whose
//    session is $LOGOS_SESSION (the module loaded there from its .lgx), and the module's reply is returned. Every
//    call is appended to $SEQNET_CALL_LOG (method, request, reply, milliseconds, logosctl's exit status).
//  - `seqnet`: the test's hands outside the view: the operator's and the relayer's steps and the devnet's clock
//    (e2e/seqnet/ops.sh), run between the user's actions; and the environment's settings (env()).
//
// Built with Qt 6 (Core, Qml, Quick, QuickTest); run as `ferrata_miniapp_logos -input tst_miniapp_logos.qml`.
#include <QtQuickTest>
#include <QDateTime>
#include <QElapsedTimer>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QProcess>
#include <QQmlContext>
#include <QQmlEngine>
#include <QTemporaryFile>

static void appendLine(const QString &path, const QJsonObject &o)
{
    if (path.isEmpty())
        return;
    QFile f(path);
    if (f.open(QIODevice::Append)) {
        f.write(QJsonDocument(o).toJson(QJsonDocument::Compact));
        f.write("\n");
    }
}

static QJsonValue parsed(const QString &text)
{
    QJsonParseError e;
    QJsonDocument d = QJsonDocument::fromJson(text.toUtf8(), &e);
    if (e.error != QJsonParseError::NoError)
        return QJsonValue(text);
    return d.isObject() ? QJsonValue(d.object()) : QJsonValue(text);
}

class LogosCore : public QObject
{
    Q_OBJECT
public:
    using QObject::QObject;
    // One call of a core module, as Logos Core's host bridge gives it to a ui module: the reply (a JSON string).
    Q_INVOKABLE QString callModule(const QString &module, const QString &method, const QVariantList &args)
    {
        const QString arg = args.isEmpty() ? QStringLiteral("{}") : args.first().toString();
        QTemporaryFile req(qEnvironmentVariable("SEQNET_TMP", QStringLiteral(".")) + QStringLiteral("/logos-req-XXXXXX.json"));
        req.open();
        req.write(arg.toUtf8());
        req.flush();
        QProcess p;
        p.setStandardInputFile(QProcess::nullDevice());
        QElapsedTimer t;
        t.start();
        p.start(qEnvironmentVariable("LOGOSCTL"),
                {QStringLiteral("--config-dir"), qEnvironmentVariable("LOGOS_SESSION"), QStringLiteral("--timeout"),
                 QStringLiteral("3600000"), QStringLiteral("call"), module, method, QStringLiteral("@") + req.fileName(),
                 QStringLiteral("--json")});
        p.waitForFinished(-1);
        const qint64 ms = t.elapsed();
        const QString out = QString::fromUtf8(p.readAllStandardOutput()).trimmed();
        QString reply;
        const QJsonValue v = parsed(out);
        if (v.isObject() && v.toObject().contains(QStringLiteral("result"))) {
            const QJsonValue r = v.toObject().value(QStringLiteral("result"));
            reply = r.isString() ? r.toString() : QString::fromUtf8(QJsonDocument(r.toObject()).toJson(QJsonDocument::Compact));
        } else {
            QJsonObject err;
            err.insert(QStringLiteral("ok"), false);
            err.insert(QStringLiteral("error"), QStringLiteral("logosctl call %1 %2: exit %3: %4 %5")
                                                     .arg(module, method)
                                                     .arg(p.exitCode())
                                                     .arg(out.left(400), QString::fromUtf8(p.readAllStandardError()).left(400)));
            reply = QString::fromUtf8(QJsonDocument(err).toJson(QJsonDocument::Compact));
        }
        QJsonObject line;
        line.insert(QStringLiteral("t"), QDateTime::currentDateTimeUtc().toString(Qt::ISODate));
        line.insert(QStringLiteral("module"), module);
        line.insert(QStringLiteral("method"), method);
        line.insert(QStringLiteral("request"), parsed(arg));
        line.insert(QStringLiteral("reply"), parsed(reply));
        line.insert(QStringLiteral("ms"), ms);
        line.insert(QStringLiteral("logosctl_exit"), p.exitCode());
        appendLine(qEnvironmentVariable("SEQNET_CALL_LOG"), line);
        return reply;
    }
};

class SeqnetDriver : public QObject
{
    Q_OBJECT
public:
    using QObject::QObject;
    // `e2e/seqnet/ops.sh <args>`: an operator's, relayer's or clock step; its exit status (output in its log).
    Q_INVOKABLE int ops(const QString &args)
    {
        QProcess p;
        p.setProcessChannelMode(QProcess::MergedChannels);
        QElapsedTimer t;
        t.start();
        p.start(QStringLiteral("bash"), {QStringLiteral("-c"), qEnvironmentVariable("SEQNET_OPS") + QStringLiteral(" ") + args});
        p.waitForFinished(-1);
        m_last = QString::fromUtf8(p.readAll());
        QJsonObject line;
        line.insert(QStringLiteral("t"), QDateTime::currentDateTimeUtc().toString(Qt::ISODate));
        line.insert(QStringLiteral("ops"), args);
        line.insert(QStringLiteral("exit"), p.exitCode());
        line.insert(QStringLiteral("ms"), t.elapsed());
        line.insert(QStringLiteral("output"), m_last.right(2000));
        appendLine(qEnvironmentVariable("SEQNET_OPS_LOG"), line);
        return p.exitCode();
    }
    Q_INVOKABLE QString last() const { return m_last; }
    Q_INVOKABLE QString env(const QString &name) const { return qEnvironmentVariable(name.toUtf8().constData()); }

private:
    QString m_last;
};

class Setup : public QObject
{
    Q_OBJECT
public slots:
    void qmlEngineAvailable(QQmlEngine *engine)
    {
        engine->rootContext()->setContextProperty(QStringLiteral("logos"), new LogosCore(engine));
        engine->rootContext()->setContextProperty(QStringLiteral("seqnet"), new SeqnetDriver(engine));
    }
};

QUICK_TEST_MAIN_WITH_SETUP(ferrata_miniapp_logos, Setup)

#include "main.moc"
