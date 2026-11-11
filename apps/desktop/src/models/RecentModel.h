#pragma once
#include <QAbstractListModel>
#include <QList>

// Recent workspaces as returned by `workspace/recent`, newest first.
class RecentModel : public QAbstractListModel
{
    Q_OBJECT
    Q_PROPERTY(int count READ rowCount NOTIFY countChanged)
public:
    enum Roles { RootRole = Qt::UserRole + 1, NameRole };
    struct Item
    {
        QString root;
        QString name;
    };

    explicit RecentModel(QObject *parent = nullptr) : QAbstractListModel(parent) {}

    int rowCount(const QModelIndex &parent = {}) const override
    {
        return parent.isValid() ? 0 : static_cast<int>(m_items.size());
    }
    QVariant data(const QModelIndex &index, int role) const override
    {
        if (!index.isValid() || index.row() >= m_items.size())
            return {};
        const Item &it = m_items.at(index.row());
        return role == RootRole ? QVariant(it.root) : role == NameRole || role == Qt::DisplayRole ? QVariant(it.name) : QVariant();
    }
    QHash<int, QByteArray> roleNames() const override
    {
        return { { RootRole, "root" }, { NameRole, "name" } };
    }
    void setItems(const QList<Item> &items)
    {
        beginResetModel();
        m_items = items;
        endResetModel();
        emit countChanged();
    }
    QList<Item> items() const { return m_items; }

signals:
    void countChanged();

private:
    QList<Item> m_items;
};
